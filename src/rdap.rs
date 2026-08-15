//! RDAP domain lookups (RFC 7483 / 9083).

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::limit::{host_of, HostLimiter};
use crate::model::Details;

/// Extra attempts per server when the registry pushes back (403/429/5xx).
const RETRIES: u32 = 2;

pub struct RdapResponse {
    /// `true` = no such object (available), `false` = registered.
    pub available: bool,
    pub server: String,
    pub details: Option<Details>,
    pub raw: Option<Value>,
}

/// Try each bootstrap server in turn; the first conclusive answer wins.
pub async fn query(
    client: &reqwest::Client,
    limiter: &HostLimiter,
    servers: &[String],
    domain: &str,
    want_raw: bool,
) -> Result<RdapResponse> {
    let mut last_err = None;

    'servers: for base in servers {
        let host = host_of(base);
        if limiter.tripped(host).await {
            last_err = Some(anyhow!("{host}: rate-limited, stopped querying it"));
            continue;
        }

        let url = format!("{base}domain/{domain}");
        let mut attempt = 0;

        let (status, resp) = loop {
            let permit = limiter.acquire(host).await;
            let resp = client
                .get(&url)
                .header("Accept", "application/rdap+json, application/json")
                .send()
                .await;
            drop(permit);

            match resp {
                Ok(r) => {
                    let status = r.status();
                    // 403/429/5xx from a registry means "slow down", not "no such domain".
                    let retryable = status.as_u16() == 403
                        || status.as_u16() == 429
                        || status.is_server_error();
                    if retryable {
                        limiter.refused(host).await;
                        if attempt < RETRIES && !limiter.tripped(host).await {
                            attempt += 1;
                            continue;
                        }
                    } else {
                        limiter.answered(host).await;
                    }
                    break (status, r);
                }
                Err(e) => {
                    if attempt < RETRIES && (e.is_timeout() || e.is_connect()) {
                        attempt += 1;
                        tokio::time::sleep(Duration::from_millis(700 * attempt as u64)).await;
                        continue;
                    }
                    last_err = Some(anyhow!("{base}: {e}"));
                    continue 'servers;
                }
            }
        };

        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(RdapResponse {
                available: true,
                server: base.clone(),
                details: None,
                raw: None,
            });
        }

        if !status.is_success() {
            last_err = Some(anyhow!("{base}: HTTP {}", status.as_u16()));
            continue;
        }

        let body: Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                last_err = Some(anyhow!("{base}: bad JSON ({e})"));
                continue;
            }
        };

        // A few servers answer 200 with an errorCode body.
        if let Some(code) = body.get("errorCode").and_then(Value::as_u64) {
            if code == 404 {
                return Ok(RdapResponse {
                    available: true,
                    server: base.clone(),
                    details: None,
                    raw: None,
                });
            }
            last_err = Some(anyhow!("{base}: RDAP error {code}"));
            continue;
        }

        // ...and others answer 200 with a non-standard error body. Only a real
        // domain object proves the name is registered; anything else is either
        // a "no such domain" in disguise or something we should not read as one.
        if !is_domain_object(&body) {
            if not_found_body(&body) {
                return Ok(RdapResponse {
                    available: true,
                    server: base.clone(),
                    details: None,
                    raw: None,
                });
            }
            last_err = Some(anyhow!("{base}: unrecognised RDAP response"));
            continue;
        }

        // Some registries answer a query for an unregistered name under a
        // delegated second level with the record of the parent (asking .in
        // about `foo.ernet.in` returns `ernet.in`). That says nothing about
        // the name we asked for, so refuse to read it either way.
        if let Some(answered) = answered_name(&body) {
            if answered != domain.to_ascii_lowercase() {
                last_err = Some(anyhow!("{base}: answered for {answered}, not {domain}"));
                continue;
            }
        }

        return Ok(RdapResponse {
            available: false,
            details: Some(parse_details(&body)),
            server: base.clone(),
            raw: want_raw.then_some(body),
        });
    }

    Err(last_err.unwrap_or_else(|| anyhow!("no RDAP server for this TLD")))
}

/// Does this body describe an actual domain registration?
fn is_domain_object(body: &Value) -> bool {
    body.get("objectClassName").and_then(Value::as_str) == Some("domain")
        || body.get("ldhName").and_then(Value::as_str).is_some()
        || body.get("handle").and_then(Value::as_str).is_some()
}

/// The name this body is actually about, lowercased and without a trailing dot.
fn answered_name(body: &Value) -> Option<String> {
    body.get("ldhName")
        .and_then(Value::as_str)
        .map(|n| n.trim_end_matches('.').to_ascii_lowercase())
}

/// Non-standard "no such domain" bodies, e.g. `.sn`:
/// `{"errors":[{"errorCode":"NOT_FOUND_DOMAIN_NAME_WITH_NAME", ...}]}`
fn not_found_body(body: &Value) -> bool {
    let text = body.to_string().to_ascii_lowercase();
    (text.contains("not_found") || text.contains("not found") || text.contains("does not exist"))
        && !text.contains("rate")
}

pub fn parse_details(body: &Value) -> Details {
    let mut d = Details {
        handle: body
            .get("handle")
            .and_then(Value::as_str)
            .map(str::to_string),
        ..Details::default()
    };

    if let Some(statuses) = body.get("status").and_then(Value::as_array) {
        d.statuses = statuses
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }

    if let Some(events) = body.get("events").and_then(Value::as_array) {
        for ev in events {
            let action = ev
                .get("eventAction")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let date = ev
                .get("eventDate")
                .and_then(Value::as_str)
                .map(str::to_string);
            match action.as_str() {
                "registration" => d.created = date,
                "expiration" => d.expires = date,
                "last changed" => d.updated = date,
                _ => {}
            }
        }
    }

    if let Some(nameservers) = body.get("nameservers").and_then(Value::as_array) {
        d.nameservers = nameservers
            .iter()
            .filter_map(|ns| ns.get("ldhName").and_then(Value::as_str))
            .map(|s| s.to_ascii_lowercase())
            .collect();
    }

    d.dnssec = body
        .get("secureDNS")
        .and_then(|s| s.get("delegationSigned"))
        .and_then(Value::as_bool);

    if let Some(entities) = body.get("entities").and_then(Value::as_array) {
        for entity in entities {
            let roles: Vec<String> = entity
                .get("roles")
                .and_then(Value::as_array)
                .map(|r| {
                    r.iter()
                        .filter_map(Value::as_str)
                        .map(|s| s.to_ascii_lowercase())
                        .collect()
                })
                .unwrap_or_default();

            if roles.iter().any(|r| r == "registrar") {
                d.registrar = vcard_field(entity, "fn");
                d.registrar_id = entity
                    .get("publicIds")
                    .and_then(Value::as_array)
                    .and_then(|ids| ids.first())
                    .and_then(|id| id.get("identifier"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                d.abuse_email = abuse_email(entity);
            } else if roles.iter().any(|r| r == "registrant") {
                d.registrant = vcard_field(entity, "fn").or_else(|| vcard_field(entity, "org"));
            }
        }
    }

    d
}

/// vcardArray: `["vcard", [["version",{},"text","4.0"], ["fn",{},"text","Name"]]]`
fn vcard_field(entity: &Value, key: &str) -> Option<String> {
    let items = entity
        .get("vcardArray")
        .and_then(Value::as_array)?
        .get(1)?
        .as_array()?;
    for item in items {
        let arr = item.as_array()?;
        if arr.first().and_then(Value::as_str) == Some(key) {
            if let Some(v) = arr.get(3).and_then(Value::as_str) {
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn abuse_email(entity: &Value) -> Option<String> {
    let sub = entity.get("entities").and_then(Value::as_array)?;
    for e in sub {
        let is_abuse = e
            .get("roles")
            .and_then(Value::as_array)
            .map(|r| {
                r.iter()
                    .filter_map(Value::as_str)
                    .any(|s| s.eq_ignore_ascii_case("abuse"))
            })
            .unwrap_or(false);
        if is_abuse {
            if let Some(email) = vcard_field(e, "email") {
                return Some(email);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        serde_json::json!({
            "handle": "1225976_DOMAIN_COM-VRSN",
            "ldhName": "apple.com",
            "status": ["client transfer prohibited"],
            "events": [
                {"eventAction": "registration", "eventDate": "1987-02-19T05:00:00Z"},
                {"eventAction": "expiration", "eventDate": "2027-02-20T05:00:00Z"},
                {"eventAction": "last changed", "eventDate": "2026-02-09T15:41:53Z"},
                {"eventAction": "last update of RDAP database", "eventDate": "2026-08-15T00:00:00Z"}
            ],
            "nameservers": [{"ldhName": "A.NS.APPLE.COM"}, {"ldhName": "B.NS.APPLE.COM"}],
            "secureDNS": {"delegationSigned": false},
            "entities": [{
                "roles": ["registrar"],
                "publicIds": [{"type": "IANA Registrar ID", "identifier": "470"}],
                "vcardArray": ["vcard", [["version", {}, "text", "4.0"], ["fn", {}, "text", "COM LAUDE"]]],
                "entities": [{
                    "roles": ["abuse"],
                    "vcardArray": ["vcard", [["email", {}, "text", "abuse@comlaude.com"]]]
                }]
            }]
        })
    }

    #[test]
    fn extracts_registration_details() {
        let d = parse_details(&sample());
        assert_eq!(d.handle.as_deref(), Some("1225976_DOMAIN_COM-VRSN"));
        assert_eq!(d.registrar.as_deref(), Some("COM LAUDE"));
        assert_eq!(d.registrar_id.as_deref(), Some("470"));
        assert_eq!(d.created.as_deref(), Some("1987-02-19T05:00:00Z"));
        assert_eq!(d.expires.as_deref(), Some("2027-02-20T05:00:00Z"));
        // "last changed" wins over "last update of RDAP database"
        assert_eq!(d.updated.as_deref(), Some("2026-02-09T15:41:53Z"));
        assert_eq!(d.statuses, vec!["client transfer prohibited"]);
        assert_eq!(d.nameservers, vec!["a.ns.apple.com", "b.ns.apple.com"]);
        assert_eq!(d.dnssec, Some(false));
        assert_eq!(d.abuse_email.as_deref(), Some("abuse@comlaude.com"));
    }

    #[test]
    fn detects_a_body_about_a_different_name() {
        // .in answers `foo.ernet.in` with the record for `ernet.in`.
        let body = serde_json::json!({"objectClassName": "domain", "ldhName": "ernet.in"});
        assert_eq!(answered_name(&body).as_deref(), Some("ernet.in"));
        assert_eq!(answered_name(&sample()).as_deref(), Some("apple.com"));
    }

    #[test]
    fn non_standard_not_found_body_is_recognised() {
        // .sn answers 200 with its own error shape.
        let body = serde_json::json!({
            "errors": [{"errorCode": "NOT_FOUND_DOMAIN_NAME_WITH_NAME",
                        "message": "No domain corresponding to x.sn has been found"}]
        });
        assert!(!is_domain_object(&body));
        assert!(not_found_body(&body));
        assert!(is_domain_object(&sample()));
    }

    #[test]
    fn empty_body_yields_empty_details() {
        assert!(parse_details(&serde_json::json!({})).is_empty());
    }
}
