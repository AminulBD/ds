//! TLD table built from the bundled `whois.json`, plus any custom table the
//! user supplies in the same format.
//!
//! The bundled file is embedded at compile time so the binary is
//! self-contained.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

const WHOIS_JSON: &str = include_str!("../whois.json");

#[derive(Debug, Deserialize)]
struct RawEntry {
    extensions: String,
    uri: String,
    available: String,
}

#[derive(Debug, Clone)]
pub enum Endpoint {
    /// Classic WHOIS over TCP port 43.
    Socket { host: String, port: u16 },
    /// Web based WHOIS: the domain is appended to the URL.
    Http { url_prefix: String },
}

impl Endpoint {
    pub fn label(&self) -> String {
        match self {
            Endpoint::Socket { host, port } => {
                if *port == 43 {
                    host.clone()
                } else {
                    format!("{host}:{port}")
                }
            }
            Endpoint::Http { url_prefix } => url_prefix.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WhoisServer {
    pub endpoint: Endpoint,
    /// Needle that marks "this domain is not registered" in the raw response.
    pub available_needle: String,
}

pub struct Registry {
    by_tld: HashMap<String, WhoisServer>,
}

impl Registry {
    pub fn load() -> Result<Self> {
        // The bundled table has a few duplicate extensions; the first wins.
        Self::parse(WHOIS_JSON, false)
    }

    /// Read a custom table in the same format as the bundled `whois.json`:
    /// a list of `{"extensions", "uri", "available"}` entries.
    pub fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading WHOIS servers from {}", path.display()))?;

        let registry = Self::parse(&text, true).with_context(|| {
            format!(
                "{}: expected a WHOIS server table, as in [{{\"extensions\": \".example\", \
                 \"uri\": \"socket://whois.example\", \"available\": \"No match\"}}]",
                path.display()
            )
        })?;

        if registry.by_tld.is_empty() {
            bail!("{}: no usable entries in the file", path.display());
        }
        Ok(registry)
    }

    /// `strict` rejects entries this parser cannot use, rather than skipping
    /// them: worth complaining about in a file the user just wrote by hand.
    fn parse(text: &str, strict: bool) -> Result<Self> {
        let raw: Vec<RawEntry> = serde_json::from_str(text)?;
        let mut by_tld = HashMap::new();

        for entry in raw {
            let endpoint = match parse_uri(&entry.uri) {
                Some(e) => e,
                None if strict => bail!(
                    "{}: expected socket://host[:port] or an http(s) URL",
                    entry.uri
                ),
                None => continue,
            };
            for ext in entry.extensions.split(',') {
                let tld = normalize_tld(ext);
                if tld.is_empty() {
                    continue;
                }
                by_tld.entry(tld).or_insert_with(|| WhoisServer {
                    endpoint: endpoint.clone(),
                    // A few entries prefix the needle with `---`; it is not part
                    // of the text the registry actually returns.
                    available_needle: entry.available.trim_start_matches('-').trim().to_string(),
                });
            }
        }

        Ok(Self { by_tld })
    }

    /// Overlay another table on this one; the other side wins per TLD.
    pub fn merge(&mut self, other: Self) {
        self.by_tld.extend(other.by_tld);
    }

    pub fn len(&self) -> usize {
        self.by_tld.len()
    }

    /// Longest-suffix lookup: `co.uk` wins over `uk` for `example.co.uk`.
    pub fn lookup(&self, tld: &str) -> Option<&WhoisServer> {
        let tld = normalize_tld(tld);
        let mut rest = tld.as_str();
        loop {
            if let Some(server) = self.by_tld.get(rest) {
                return Some(server);
            }
            rest = &rest[rest.find('.')? + 1..];
        }
    }

    pub fn all_tlds(&self) -> BTreeSet<String> {
        self.by_tld.keys().cloned().collect()
    }
}

impl Default for Registry {
    /// An empty table, for `--whois-mode only`.
    fn default() -> Self {
        Self {
            by_tld: HashMap::new(),
        }
    }
}

/// `.CO.UK` / `co.uk` / `.com` -> `co.uk` / `com`
pub fn normalize_tld(s: &str) -> String {
    s.trim().trim_matches('.').to_ascii_lowercase()
}

fn parse_uri(uri: &str) -> Option<Endpoint> {
    if let Some(rest) = uri.strip_prefix("socket://") {
        let rest = rest.trim_end_matches('/');
        let (host, port) = match rest.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().unwrap_or(43)),
            None => (rest.to_string(), 43),
        };
        if host.is_empty() {
            return None;
        }
        Some(Endpoint::Socket { host, port })
    } else if uri.starts_with("http://") || uri.starts_with("https://") {
        Some(Endpoint::Http {
            url_prefix: uri.to_string(),
        })
    } else {
        None
    }
}

/// A small hand-picked set for `--tld popular`.
pub const POPULAR: &[&str] = &[
    "com", "net", "org", "io", "co", "ai", "app", "dev", "sh", "me", "info", "biz", "xyz",
    "online", "site", "tech", "store", "cloud", "space", "live", "world", "life", "us", "uk",
    "co.uk", "de", "fr", "nl", "es", "it", "eu", "ca", "com.au", "in", "jp", "cn", "br", "ru",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_tlds() {
        assert_eq!(normalize_tld(".CO.UK"), "co.uk");
        assert_eq!(normalize_tld(" com "), "com");
    }

    #[test]
    fn longest_suffix_lookup() {
        let r = Registry::load().unwrap();
        // .uk and .co.uk share a server; an unknown sub-label falls back to .uk
        assert!(r.lookup("co.uk").is_some());
        assert!(r.lookup("nosuchtld12345").is_none());
        let com = r.lookup("com").unwrap();
        assert!(matches!(&com.endpoint, Endpoint::Socket { port: 43, .. }));
    }

    #[test]
    fn strips_dashes_from_needles() {
        let r = Registry::load().unwrap();
        assert_eq!(r.lookup("ch").unwrap().available_needle, "1:");
        assert_eq!(r.lookup("io").unwrap().available_needle, "Domain not found");
    }

    fn write(name: &str, body: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn reads_a_custom_table() {
        let path = write(
            "ds-whois-custom.json",
            r#"[{"extensions": ".Internal,.corp", "uri": "socket://whois.corp.invalid:4343",
                 "available": "not registered"}]"#,
        );
        let r = Registry::from_file(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(r.len(), 2);
        let s = r.lookup("corp").unwrap();
        assert_eq!(s.available_needle, "not registered");
        assert!(matches!(&s.endpoint, Endpoint::Socket { port: 4343, .. }));
    }

    #[test]
    fn custom_entries_win_when_merged() {
        let mut bundled = Registry::load().unwrap();
        let path = write(
            "ds-whois-override.json",
            r#"[{"extensions": ".com", "uri": "socket://whois.mine", "available": "free"}]"#,
        );
        bundled.merge(Registry::from_file(&path).unwrap());
        std::fs::remove_file(&path).ok();

        assert_eq!(
            bundled.lookup("com").unwrap().endpoint.label(),
            "whois.mine"
        );
        // Untouched TLDs still come from the bundled table.
        assert_eq!(bundled.lookup("ch").unwrap().available_needle, "1:");
    }

    #[test]
    fn rejects_unusable_tables() {
        for body in [
            r#"[{"extensions": ".x", "uri": "ftp://nope", "available": "no"}]"#,
            r#"[]"#,
            r#"{"com": "whois.example"}"#,
            "not json",
        ] {
            let path = write("ds-whois-bad.json", body);
            assert!(Registry::from_file(&path).is_err(), "{body}");
            std::fs::remove_file(&path).ok();
        }
    }

    #[test]
    fn parses_socket_port() {
        match parse_uri("socket://whois.nic.ch:4343").unwrap() {
            Endpoint::Socket { host, port } => {
                assert_eq!(host, "whois.nic.ch");
                assert_eq!(port, 4343);
            }
            _ => panic!("expected socket endpoint"),
        }
    }
}
