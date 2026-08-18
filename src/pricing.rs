//! Registration prices per TLD, from the bundled `pricing.json`.
//!
//! The file lists one entry per registrar per TLD, so a TLD's price is the
//! mean over the registrars that quote one. Amounts are USD, as published by
//! the registrars; they are a snapshot, not a quote.
//!
//! Like `whois.json`, the file is embedded at compile time so the binary stays
//! self-contained; a custom table in the same format can be loaded over it.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::tlds::normalize_tld;

const PRICING_JSON: &str = include_str!("../pricing.json");

#[derive(Debug, Deserialize)]
struct RawOffer {
    /// Every price key is optional so a hand-written table can quote only the
    /// figure it cares about.
    #[serde(default)]
    prices: RawPrices,
}

#[derive(Debug, Default, Deserialize)]
struct RawPrices {
    /// List price for the first year. Null where the registrar does not sell
    /// the TLD directly.
    #[serde(default)]
    regular: Option<f64>,
    #[serde(default)]
    renew: Option<f64>,
}

/// What a TLD costs, averaged over the registrars in the table.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Price {
    /// Mean first-year registration price.
    pub register: f64,
    /// Mean renewal price, when any registrar quotes one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renew: Option<f64>,
    pub currency: &'static str,
    /// How many registrars went into those means.
    pub registrars: usize,
}

impl Price {
    /// `$14.98`, for the price column.
    pub fn label(&self) -> String {
        format!("${:.2}", self.register)
    }
}

pub struct Prices {
    by_tld: HashMap<String, Price>,
}

impl Prices {
    pub fn load() -> Result<Self> {
        Self::parse(PRICING_JSON)
    }

    /// Read a custom table in the same format as the bundled `pricing.json`:
    /// a TLD -> registrar offers map.
    pub fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading prices from {}", path.display()))?;

        let prices = Self::parse(&text).with_context(|| {
            format!(
                "{}: expected a price table, as in {{\"example\": [{{\"register\": \
                 \"registrar.example\", \"prices\": {{\"regular\": 9.99, \"renew\": 12.99}}}}]}}",
                path.display()
            )
        })?;

        if prices.by_tld.is_empty() {
            bail!("{}: no usable entries in the file", path.display());
        }
        Ok(prices)
    }

    fn parse(text: &str) -> Result<Self> {
        let raw: HashMap<String, Vec<RawOffer>> = serde_json::from_str(text)?;
        let mut by_tld = HashMap::new();

        for (tld, offers) in raw {
            let tld = normalize_tld(&tld);
            if tld.is_empty() {
                continue;
            }
            if let Some(price) = average(&offers) {
                by_tld.insert(tld, price);
            }
        }

        Ok(Self { by_tld })
    }

    /// Longest-suffix lookup, as for WHOIS servers: `co.uk` wins over `uk`.
    pub fn lookup(&self, tld: &str) -> Option<Price> {
        let tld = normalize_tld(tld);
        let mut rest = tld.as_str();
        loop {
            if let Some(price) = self.by_tld.get(rest) {
                return Some(*price);
            }
            rest = &rest[rest.find('.')? + 1..];
        }
    }

    /// Overlay another table on this one; the other side wins per TLD.
    pub fn merge(&mut self, other: Self) {
        self.by_tld.extend(other.by_tld);
    }

    pub fn len(&self) -> usize {
        self.by_tld.len()
    }
}

impl Default for Prices {
    /// An empty table, for `--pricing-mode only`.
    fn default() -> Self {
        Self {
            by_tld: HashMap::new(),
        }
    }
}

/// Mean over the registrars that quote a price; a TLD nobody sells has none.
fn average(offers: &[RawOffer]) -> Option<Price> {
    // A price that is missing, negative or not a number is no quote at all.
    let quoted = |v: Option<f64>| v.filter(|p| p.is_finite() && *p >= 0.0);

    let register: Vec<f64> = offers
        .iter()
        .filter_map(|o| quoted(o.prices.regular))
        .collect();
    if register.is_empty() {
        return None;
    }
    let renew: Vec<f64> = offers
        .iter()
        .filter_map(|o| quoted(o.prices.renew))
        .collect();

    Some(Price {
        register: mean(&register)?,
        renew: mean(&renew),
        currency: "USD",
        registrars: register.len(),
    })
}

fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_table_loads() {
        let p = Prices::load().unwrap();
        assert!(p.len() > 400, "{} TLDs priced", p.len());
        assert!(p.lookup("com").unwrap().register > 0.0);
    }

    #[test]
    fn averages_over_registrars() {
        let p = Prices::parse(
            r#"{"com": [{"register": "a.example", "prices": {"regular": 10.0, "renew": 20.0}},
                        {"register": "b.example", "prices": {"regular": 20.0, "renew": null}}]}"#,
        )
        .unwrap();

        let com = p.lookup("com").unwrap();
        assert_eq!(com.register, 15.0);
        assert_eq!(com.renew, Some(20.0));
        assert_eq!(com.registrars, 2);
        assert_eq!(com.label(), "$15.00");
    }

    #[test]
    fn skips_tlds_nobody_prices() {
        let p = Prices::parse(
            r#"{"nu": [{"register": "a.example", "prices": {"regular": null, "renew": null}}]}"#,
        )
        .unwrap();
        assert!(p.lookup("nu").is_none());
    }

    #[test]
    fn longest_suffix_wins() {
        let p = Prices::parse(
            r#"{"uk": [{"register": "a.example", "prices": {"regular": 7.0, "renew": 9.0}}],
                "co.uk": [{"register": "a.example", "prices": {"regular": 5.0, "renew": 6.0}}]}"#,
        )
        .unwrap();

        assert_eq!(p.lookup("co.uk").unwrap().register, 5.0);
        // An unlisted sub-zone falls back to the TLD it sits under.
        assert_eq!(p.lookup("nosuchzone.uk").unwrap().register, 7.0);
        assert!(p.lookup("nosuchtld12345").is_none());
    }

    fn write(name: &str, body: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn reads_a_custom_table() {
        let path = write(
            "ds-pricing-custom.json",
            r#"{".Internal": [{"register": "corp.example", "prices": {"regular": 3.5}}]}"#,
        );
        let p = Prices::from_file(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(p.len(), 1);
        let internal = p.lookup("internal").unwrap();
        assert_eq!(internal.label(), "$3.50");
        // Nothing quoted a renewal, so there is none to report.
        assert_eq!(internal.renew, None);
    }

    #[test]
    fn custom_entries_win_when_merged() {
        let mut bundled = Prices::load().unwrap();
        let bundled_io = bundled.lookup("io").unwrap().register;
        let path = write(
            "ds-pricing-override.json",
            r#"{"com": [{"register": "mine.example", "prices": {"regular": 1.0}}]}"#,
        );
        bundled.merge(Prices::from_file(&path).unwrap());
        std::fs::remove_file(&path).ok();

        assert_eq!(bundled.lookup("com").unwrap().register, 1.0);
        // Untouched TLDs still come from the bundled table.
        assert_eq!(bundled.lookup("io").unwrap().register, bundled_io);
    }

    #[test]
    fn rejects_unusable_tables() {
        for body in [
            // Nothing priced at all.
            r#"{"com": [{"register": "a.example", "prices": {"regular": null}}]}"#,
            r#"{"com": []}"#,
            r#"{}"#,
            // The WHOIS table format, pointed at the wrong option.
            r#"[{"extensions": ".com", "uri": "socket://whois.example", "available": "free"}]"#,
            r#"{"com": {"regular": 9.99}}"#,
            "not json",
        ] {
            let path = write("ds-pricing-bad.json", body);
            assert!(Prices::from_file(&path).is_err(), "{body}");
            std::fs::remove_file(&path).ok();
        }
    }

    #[test]
    fn ignores_prices_that_are_not_quotes() {
        let p = Prices::parse(
            r#"{"com": [{"register": "a.example", "prices": {"regular": -1.0, "renew": 12.0}},
                        {"register": "b.example", "prices": {"regular": 8.0, "renew": 10.0}}]}"#,
        )
        .unwrap();

        let com = p.lookup("com").unwrap();
        assert_eq!(com.register, 8.0, "the negative price is not averaged in");
        assert_eq!(com.registrars, 1);
        assert_eq!(com.renew, Some(11.0));
    }

    #[test]
    fn normalises_the_looked_up_tld() {
        let p = Prices::load().unwrap();
        assert_eq!(
            p.lookup(".COM").map(|c| c.label()),
            p.lookup("com").map(|c| c.label())
        );
    }
}
