//! IANA RDAP bootstrap (https://data.iana.org/rdap/dns.json), cached on disk.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
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
                let urls: Vec<String> = urls
                    .iter()
                    .map(|u| {
                        if u.ends_with('/') {
                            u.clone()
                        } else {
                            format!("{u}/")
                        }
                    })
                    .collect();
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
}

fn cache_path() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("dc").join("rdap-dns.json")
}
