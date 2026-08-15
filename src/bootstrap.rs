//! RDAP server lists: the IANA bootstrap
//! (https://data.iana.org/rdap/dns.json, cached on disk) and any custom
//! `rdap.json` the user supplies.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

const BOOTSTRAP_URL: &str = "https://data.iana.org/rdap/dns.json";
const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug, Deserialize)]
struct BootstrapFile {
    /// `[[["com","net"], ["https://rdap.verisign.com/com/v1/"]], ...]`
    services: Vec<Vec<Vec<String>>>,
}

#[derive(Debug, Default)]
pub struct Bootstrap {
    by_tld: HashMap<String, Vec<String>>,
}

impl Bootstrap {
    pub async fn load(client: &reqwest::Client, refresh: bool) -> Result<Self> {
        let path = cache_path();
        let fresh = !refresh
            && path
                .metadata()
                .and_then(|m| m.modified())
                .map(|m| SystemTime::now().duration_since(m).unwrap_or(MAX_AGE) < MAX_AGE)
                .unwrap_or(false);

        if fresh {
            if let Ok(text) = tokio::fs::read_to_string(&path).await {
                if let Ok(b) = Self::parse(&text) {
                    return Ok(b);
                }
            }
        }

        match Self::fetch(client).await {
            Ok(text) => {
                let parsed = Self::parse(&text)?;
                if let Some(dir) = path.parent() {
                    let _ = tokio::fs::create_dir_all(dir).await;
                }
                let _ = tokio::fs::write(&path, &text).await;
                Ok(parsed)
            }
            Err(e) => {
                // Network down: fall back to a stale cache if we have one.
                if let Ok(text) = tokio::fs::read_to_string(&path).await {
                    if let Ok(b) = Self::parse(&text) {
                        return Ok(b);
                    }
                }
                Err(e)
            }
        }
    }

    async fn fetch(client: &reqwest::Client) -> Result<String> {
        let resp = client
            .get(BOOTSTRAP_URL)
            .send()
            .await
            .context("fetching IANA RDAP bootstrap")?
            .error_for_status()?;
        Ok(resp.text().await?)
    }

    fn parse(text: &str) -> Result<Self> {
        let file: BootstrapFile = serde_json::from_str(text)?;
        let mut by_tld: HashMap<String, Vec<String>> = HashMap::new();
        for service in file.services {
            let (tlds, urls) = match (service.first(), service.get(1)) {
                (Some(t), Some(u)) => (t, u),
                _ => continue,
            };
            for tld in tlds {
                let urls: Vec<String> = urls.iter().cloned().map(normalize_url).collect();
                by_tld.insert(tld.to_ascii_lowercase(), urls);
            }
        }
        Ok(Self { by_tld })
    }

    /// Longest-suffix match, so `co.uk` resolves through the `uk` service.
    pub fn servers_for(&self, tld: &str) -> Option<&[String]> {
        let tld = tld.trim_matches('.').to_ascii_lowercase();
        let mut rest = tld.as_str();
        loop {
            if let Some(urls) = self.by_tld.get(rest) {
                return Some(urls);
            }
            rest = &rest[rest.find('.')? + 1..];
        }
    }

    pub fn all_tlds(&self) -> BTreeSet<String> {
        self.by_tld.keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.by_tld.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_tld.len()
    }

    /// Read a custom server list in the RDAP bootstrap format (RFC 9224),
    /// the same shape as IANA's `dns.json`, so a copy of that file can be
    /// edited and handed straight back.
    pub fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading RDAP servers from {}", path.display()))?;

        let parsed = Self::parse(&text).with_context(|| {
            format!(
                "{}: expected an RDAP bootstrap file, as in \
                 {{\"services\": [[[\"com\"], [\"https://rdap.example/\"]]]}}",
                path.display()
            )
        })?;

        for urls in parsed.by_tld.values() {
            for url in urls {
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    bail!("{}: {url} is not an http(s) URL", path.display());
                }
            }
        }
        if parsed.by_tld.is_empty() {
            bail!("{}: no services in the file", path.display());
        }

        Ok(parsed)
    }

    /// Overlay another list on this one; the other side wins per TLD.
    pub fn merge(&mut self, other: Self) {
        self.by_tld.extend(other.by_tld);
    }
}

/// RDAP base URLs are joined with `domain/<name>`, so they need a trailing slash.
fn normalize_url(url: String) -> String {
    if url.ends_with('/') {
        url
    } else {
        format!("{url}/")
    }
}

fn cache_path() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("ds").join("rdap-dns.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, body: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn reads_the_bootstrap_format() {
        let path = write(
            "ds-rdap-services.json",
            r#"{"version":"1.0","services":[[["COM","net"],["https://a.example/rdap"]]]}"#,
        );
        let b = Bootstrap::from_file(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // Keys are lowercased and base URLs get the trailing slash they need.
        assert_eq!(
            b.servers_for("com"),
            Some(&["https://a.example/rdap/".to_string()][..])
        );
        assert_eq!(b.servers_for("net").unwrap().len(), 1);
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn custom_entries_win_when_merged() {
        let iana = Bootstrap::parse(
            r#"{"services":[[["com"],["https://iana/"]],[["net"],["https://iana/"]]]}"#,
        )
        .unwrap();
        let path = write(
            "ds-rdap-override.json",
            r#"{"services":[[["com"],["https://mine/"]]]}"#,
        );
        let custom = Bootstrap::from_file(&path).unwrap();
        std::fs::remove_file(&path).ok();

        let mut merged = iana;
        merged.merge(custom);
        assert_eq!(
            merged.servers_for("com"),
            Some(&["https://mine/".to_string()][..])
        );
        assert_eq!(
            merged.servers_for("net"),
            Some(&["https://iana/".to_string()][..])
        );
    }

    #[test]
    fn rejects_unusable_entries() {
        for body in [
            r#"{"com": 42}"#,
            r#"{"com": "ftp://x"}"#,
            r#"{"com": []}"#,
            r#"["com"]"#,
        ] {
            let path = write("ds-rdap-bad.json", body);
            assert!(Bootstrap::from_file(&path).is_err(), "{body}");
            std::fs::remove_file(&path).ok();
        }
    }
}
