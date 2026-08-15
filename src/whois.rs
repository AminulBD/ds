//! WHOIS queries: port 43 sockets plus the handful of web-form registries
//! that `dist.whois.json` lists with an http(s) URI.

use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::limit::HostLimiter;
use crate::model::{Details, Status};
use crate::tlds::{Endpoint, WhoisServer};

pub struct WhoisResponse {
    pub server: String,
    pub raw: String,
    pub status: Status,
    pub note: Option<String>,
}

pub async fn query(
    client: &reqwest::Client,
    limiter: &HostLimiter,
    server: &WhoisServer,
    domain: &str,
    timeout: Duration,
) -> Result<WhoisResponse> {
    let raw = match &server.endpoint {
        Endpoint::Socket { host, port } => {
            if limiter.tripped(host).await {
                anyhow::bail!("{host}: rate-limited, stopped querying it");
            }
            let mut attempt = 0;
            loop {
                let permit = limiter.acquire(host).await;
                let result = socket_query(host, *port, domain, timeout).await;
                drop(permit);
                match result {
                    Ok(raw) => {
                        limiter.answered(host).await;
                        break raw;
                    }
                    Err(e) => {
                        limiter.refused(host).await;
                        // Only a reset connection is worth retrying — that is
                        // how registries signal "too fast". A refused
                        // connection, dead host or timeout will not improve.
                        let reset = e.chain().any(|c| {
                            c.downcast_ref::<std::io::Error>()
                                .is_some_and(|io| io.kind() == std::io::ErrorKind::ConnectionReset)
                        });
                        if !reset || attempt >= 1 || limiter.tripped(host).await {
                            return Err(e);
                        }
                        attempt += 1;
                    }
                }
            }
        }
        Endpoint::Http { url_prefix } => {
            let url = format!("{url_prefix}{domain}");
            let permit = limiter.acquire(crate::limit::host_of(url_prefix)).await;
            let body = client
                .get(&url)
                .timeout(timeout)
                .send()
                .await
                .with_context(|| format!("GET {url}"))?
                .text()
                .await?;
            drop(permit);
            strip_html(&body)
        }
    };

    let (status, note) = classify(&raw, &server.available_needle, domain);
    Ok(WhoisResponse {
        server: server.endpoint.label(),
        raw,
        status,
        note,
    })
}

async fn socket_query(host: &str, port: u16, domain: &str, timeout: Duration) -> Result<String> {
    let addr = format!("{host}:{port}");
    let mut stream = tokio::time::timeout(timeout, TcpStream::connect(&addr))
        .await
        .with_context(|| format!("connecting to {addr}: timed out"))?
        .with_context(|| format!("connecting to {addr}"))?;

    let query = format_query(host, domain);
    tokio::time::timeout(timeout, stream.write_all(query.as_bytes()))
        .await
        .context("write timed out")??;
    stream.flush().await.ok();

    let mut buf = Vec::new();
    tokio::time::timeout(timeout, stream.read_to_end(&mut buf))
        .await
        .with_context(|| format!("reading from {addr}: timed out"))??;

    Ok(String::from_utf8_lossy(&buf).replace("\r\n", "\n"))
}

/// Ask IANA which WHOIS server currently serves a TLD.
///
/// `dist.whois.json` is a snapshot and some of its hosts no longer resolve
/// (`whois.nic.co`, `whois.nic.tr`, ...); IANA's own WHOIS knows the current
/// one. Only the last label is meaningful to IANA, so `co.za` is asked as `za`.
pub async fn iana_referral(
    limiter: &HostLimiter,
    tld: &str,
    timeout: Duration,
) -> Result<Option<String>> {
    let apex = tld.rsplit('.').next().unwrap_or(tld);
    let permit = limiter.acquire("whois.iana.org").await;
    let raw = socket_query("whois.iana.org", 43, apex, timeout).await;
    drop(permit);

    let raw = raw?;
    for line in raw.lines() {
        if let Some(value) = line.trim().strip_prefix("whois:") {
            let host = value.trim();
            if !host.is_empty() {
                return Ok(Some(host.to_ascii_lowercase()));
            }
        }
    }
    Ok(None)
}

/// A WHOIS server discovered through IANA, judged by generic markers only.
pub fn referred_server(host: String) -> WhoisServer {
    WhoisServer {
        endpoint: Endpoint::Socket { host, port: 43 },
        available_needle: String::new(),
    }
}

/// Some registries need a query prefix/suffix rather than a bare domain.
fn format_query(host: &str, domain: &str) -> String {
    let h = host.to_ascii_lowercase();
    if h.contains("verisign-grs") || h.contains("crsnic") || h.contains("internic") {
        // Ask for the registry record rather than a fuzzy match.
        format!("domain {domain}\r\n")
    } else if h.contains("denic") {
        format!("-T dn {domain}\r\n")
    } else if h.contains("jprs") {
        format!("{domain}/e\r\n")
    } else if h.contains("whois.arnes.si") || h.contains("dk-hostmaster") {
        format!("--show-handles {domain}\r\n")
    } else {
        format!("{domain}\r\n")
    }
}

/// Markers that mean "the registry answered, and the name exists".
const TAKEN_MARKERS: &[&str] = &[
    "domain name:",
    "domain:",
    "registrar:",
    "creation date",
    "created:",
    "registered on",
    "expiry date",
    "expiration date",
    "registry expiry date",
    "name server",
    "nserver",
    "status: connect",
    "status: active",
    "already registered",
    "can not be registered",
    "cannot be registered",
    "is not available for registration",
];

/// Unambiguous "this object does not exist" statements. Several registries
/// echo the queried name back (`Domain: x.sr` / `Message: No Object Found`),
/// so these must outrank the markers above.
const STRONG_FREE_MARKERS: &[&str] = &[
    "no match for",
    "no matching record",
    "no object found",
    "does not exist",
    "no such domain",
    "domain not found",
    "no entries found",
    "no data found",
    "not registered",
    "status: free",
    "status: available",
    "available for registration",
];

/// Weaker "not registered" phrasings, used when nothing above matched.
const FREE_MARKERS: &[&str] = &[
    "no match",
    "not found",
    "is available",
    "no information available",
    "nothing found",
];

/// The server cannot answer for this TLD at all. Some of these responses echo
/// the queried name back as `Domain Name: ...`, which would otherwise read as
/// a registration, so they are checked before anything else.
const REFUSAL_MARKERS: &[&str] = &[
    "tld is not supported",
    "tld not supported",
    "not supported by this registry",
    "this tld has no whois server",
    "no whois server is known",
    "access denied",
    // Narrow on purpose: a bare "not permitted" also appears in the terms of
    // use that registries such as DENIC prepend to every answer.
    "requests of this client are not permitted",
    "queries are not permitted",
    "status: not allowed",
    "use only approved characters",
    "domain status: error",
    "not allowed for this domain category",
];

/// Rate limiting / transient server errors.
const ERROR_MARKERS: &[&str] = &[
    "limit exceeded",
    "rate limit",
    "too many requests",
    "try again later",
    "quota exceeded",
    "connection refused",
    "service unavailable",
    "temporarily unavailable",
    "blocked",
];

fn classify(raw: &str, needle: &str, domain: &str) -> (Status, Option<String>) {
    // Registries pad keys with tabs and runs of spaces; normalise so that
    // `Status:\tNOT ALLOWED` matches the same marker as `Status: not allowed`.
    let lower = normalize(raw);
    let needle_lower = normalize(needle);

    if !needle_lower.is_empty() && lower.contains(&needle_lower) {
        return (Status::Available, None);
    }

    if let Some(marker) = REFUSAL_MARKERS.iter().find(|m| lower.contains(**m)) {
        return (Status::Unknown, Some(format!("whois said: {marker}")));
    }

    if let Some(marker) = ERROR_MARKERS.iter().find(|m| lower.contains(**m)) {
        // Only treat it as an error when the response carries nothing useful.
        if !TAKEN_MARKERS.iter().any(|m| lower.contains(m)) {
            return (Status::Unknown, Some(format!("whois said: {marker}")));
        }
    }

    if STRONG_FREE_MARKERS.iter().any(|m| lower.contains(m)) {
        return (Status::Available, None);
    }

    // `.in` answers a query for an unregistered name under a delegated second
    // level with the parent's record. A record about another name proves
    // nothing about this one.
    if let Some(answered) = answered_name(raw) {
        if answered != domain.to_ascii_lowercase() {
            return (
                Status::Unknown,
                Some(format!("whois answered for {answered}")),
            );
        }
    }

    if TAKEN_MARKERS.iter().any(|m| lower.contains(m)) {
        return (Status::Taken, None);
    }

    if FREE_MARKERS.iter().any(|m| lower.contains(m)) {
        return (Status::Available, None);
    }

    if lower.trim().is_empty() {
        return (Status::Unknown, Some("empty whois response".into()));
    }

    (Status::Unknown, Some("unrecognised whois response".into()))
}

/// The domain a WHOIS record is about, if it names one.
fn answered_name(raw: &str) -> Option<String> {
    for line in raw.lines() {
        let (key, value) = line.trim().split_once(':')?;
        let key = key.trim().to_ascii_lowercase();
        if matches!(
            key.as_str(),
            "domain name" | "domain" | "domainname" | "internationalized domain name"
        ) {
            let value = value.trim().trim_end_matches('.').to_ascii_lowercase();
            if !value.is_empty() && value.contains('.') {
                return Some(value);
            }
        }
    }
    None
}

/// Lowercase and collapse runs of horizontal whitespace, so marker matching
/// does not depend on how a registry formats its key/value columns.
fn normalize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_space = false;
    for c in raw.chars() {
        let c = if c == '\t' { ' ' } else { c };
        if c == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            last_space = false;
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// Parse the common `Key: Value` WHOIS layout into structured details.
pub fn parse_details(raw: &str) -> Details {
    let mut d = Details::default();

    for line in raw.lines() {
        let line = line.trim();
        let (key, value) = match line.split_once(':') {
            Some((k, v)) => (k.trim().to_ascii_lowercase(), v.trim()),
            None => continue,
        };
        if value.is_empty() {
            continue;
        }

        match key.as_str() {
            "registrar" | "sponsoring registrar" | "registrar name" if d.registrar.is_none() => {
                d.registrar = Some(value.to_string())
            }
            "registrar iana id" if d.registrar_id.is_none() => {
                d.registrar_id = Some(value.to_string())
            }
            "registrant" | "registrant name" | "registrant organization" | "org"
                if d.registrant.is_none() =>
            {
                d.registrant = Some(value.to_string())
            }
            "creation date"
            | "created"
            | "created on"
            | "created date"
            | "registered on"
            | "registration time"
            | "domain registration date"
                if d.created.is_none() =>
            {
                d.created = Some(value.to_string())
            }
            "updated date" | "last updated" | "last modified" | "changed" | "modified"
                if d.updated.is_none() =>
            {
                d.updated = Some(value.to_string())
            }
            "registry expiry date"
            | "expiry date"
            | "expiration date"
            | "expires"
            | "expires on"
            | "paid-till"
            | "renewal date"
                if d.expires.is_none() =>
            {
                d.expires = Some(value.to_string())
            }
            "domain status" | "status" | "state" => {
                let v = value.to_string();
                if !d.statuses.contains(&v) {
                    d.statuses.push(v);
                }
            }
            "name server" | "nserver" | "nameserver" | "name servers" => {
                let ns = value
                    .split_whitespace()
                    .next()
                    .unwrap_or(value)
                    .trim_end_matches('.')
                    .to_ascii_lowercase();
                if !ns.is_empty() && !d.nameservers.contains(&ns) {
                    d.nameservers.push(ns);
                }
            }
            "dnssec" if d.dnssec.is_none() => {
                let v = value.to_ascii_lowercase();
                d.dnssec = Some(v.starts_with("signed") || v == "yes" || v == "active");
            }
            "registrar abuse contact email" if d.abuse_email.is_none() => {
                d.abuse_email = Some(value.to_string())
            }
            _ => {}
        }
    }

    d
}

/// Crude tag stripper for the few web-form WHOIS endpoints.
fn strip_html(body: &str) -> String {
    if !body.contains('<') {
        return body.to_string();
    }
    // Drop <script>/<style> bodies wholesale, then strip the remaining tags.
    let mut cleaned = body.to_string();
    for tag in ["script", "style"] {
        loop {
            let lower = cleaned.to_ascii_lowercase();
            let open = match lower.find(&format!("<{tag}")) {
                Some(i) => i,
                None => break,
            };
            let close = match lower[open..].find(&format!("</{tag}")) {
                Some(i) => open + i,
                None => cleaned.len(),
            };
            let end = lower[close..]
                .find('>')
                .map(|i| close + i + 1)
                .unwrap_or(cleaned.len());
            cleaned.replace_range(open..end, "\n");
        }
    }

    let mut out = String::with_capacity(cleaned.len());
    let mut in_tag = false;
    for c in cleaned.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }

    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needle_match_wins() {
        let (s, _) = classify("Domain not found.\n", "Domain not found", "x.test");
        assert_eq!(s, Status::Available);
    }

    #[test]
    fn registered_record_is_taken() {
        let raw = "Domain Name: apple.com\nRegistrar: COM LAUDE\nCreation Date: 1987-02-19\n";
        assert_eq!(classify(raw, "No match for", "apple.com").0, Status::Taken);
    }

    #[test]
    fn terse_ch_style_responses() {
        assert_eq!(
            classify("1: This domain name can be registered.", "1:", "x.test").0,
            Status::Available
        );
        assert_eq!(
            classify("0: This domain name can not be registered.", "1:", "x.test").0,
            Status::Taken
        );
    }

    #[test]
    fn generic_free_phrasing_without_needle() {
        assert_eq!(
            classify("NOT FOUND\n", "no match", "x.test").0,
            Status::Available
        );
    }

    #[test]
    fn dead_server_is_unknown() {
        let (s, note) = classify("TLD is not supported.\n", "No match", "x.test");
        assert_eq!(s, Status::Unknown);
        assert!(note.unwrap().contains("not supported"));
    }

    #[test]
    fn refusal_that_echoes_the_query_is_not_taken() {
        // Tucows answers for TLDs it does not serve by echoing the name back.
        let raw = "Domain Name: zqxwvu7391aminul.tattoo\n\n                   >>> This name is not available for registration:\n\n                   >>> Tld not supported by this registry interface\n";
        assert_eq!(classify(raw, "No match", "x.test").0, Status::Unknown);
    }

    #[test]
    fn echoed_name_with_no_object_found_is_available() {
        // .sr and .fj echo the query back before saying it does not exist.
        let raw = "Domain: zqxwvu7391aminul.sr\nMessage: No Object Found\n\
                   Sponsoring Registrar: Datasur\n";
        assert_eq!(classify(raw, "No match", "x.test").0, Status::Available);
    }

    #[test]
    fn query_rejection_is_unknown_despite_tabs() {
        let raw =
            "Domain:\tx.ac.be\nStatus:\tNOT ALLOWED\nMessage:\tUse only approved characters.\n";
        assert_eq!(
            classify(raw, "Status: AVAILABLE", "x.test").0,
            Status::Unknown
        );
    }

    #[test]
    fn teleinfo_style_no_matching_record_is_available() {
        let raw = "Internationalized Domain Name: x.anquan\nNo matching record.\n";
        assert_eq!(classify(raw, "No match for", "x.test").0, Status::Available);
    }

    #[test]
    fn registry_error_status_is_unknown() {
        let raw = "Domain Name: x.ac.bd\nDomain Status: Error\n\
                   Message: Reseller not allowed for this domain category\n";
        assert_eq!(classify(raw, "No match", "x.test").0, Status::Unknown);
    }

    #[test]
    fn record_about_the_parent_zone_is_unknown() {
        // .in answers `foo.ernet.in` with the record for `ernet.in`.
        let raw = "Domain Name: ernet.in\nRegistrar: National Informatics Centre\n\
                   Creation Date: 2005-02-19T06:12:14.398Z\n";
        let (status, note) = classify(raw, "No match", "foo.ernet.in");
        assert_eq!(status, Status::Unknown);
        assert!(note.unwrap().contains("ernet.in"));
    }

    #[test]
    fn boilerplate_does_not_look_like_a_refusal() {
        // DENIC prepends terms of use containing "not permitted" to every answer.
        let raw = "% Restricted rights.\n\
                   % It is not permitted to use this data for advertising.\n\
                   Domain: apple.de\nStatus: connect\n";
        assert_eq!(classify(raw, "Status: free", "apple.de").0, Status::Taken);
    }

    #[test]
    fn parses_key_value_details() {
        let raw = "Domain Name: apple.com\n\
                   Registrar: Nom-iq Ltd.\n\
                   Registrar IANA ID: 470\n\
                   Creation Date: 1987-02-19T05:00:00Z\n\
                   Registry Expiry Date: 2027-02-20T05:00:00Z\n\
                   Domain Status: clientTransferProhibited\n\
                   Name Server: A.NS.APPLE.COM\n\
                   Name Server: B.NS.APPLE.COM\n\
                   DNSSEC: unsigned\n";
        let d = parse_details(raw);
        assert_eq!(d.registrar.as_deref(), Some("Nom-iq Ltd."));
        assert_eq!(d.registrar_id.as_deref(), Some("470"));
        assert_eq!(d.created.as_deref(), Some("1987-02-19T05:00:00Z"));
        assert_eq!(d.expires.as_deref(), Some("2027-02-20T05:00:00Z"));
        assert_eq!(d.nameservers, vec!["a.ns.apple.com", "b.ns.apple.com"]);
        assert_eq!(d.dnssec, Some(false));
    }

    #[test]
    fn strips_html_wrappers() {
        let html = "<html><style>b{}</style><body><p>Domain is available</p>\
                    <script>x()</script></body></html>";
        let text = strip_html(html);
        assert_eq!(text, "Domain is available");
    }
}
