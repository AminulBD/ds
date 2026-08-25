//! Who sells a TLD, and who is allowed to buy one.
//!
//! `--where` used to answer the first question by printing the same four
//! registrar searches for every TLD, which is a guess: `.fr` is not sold by
//! Porkbun, `.edu` is not sold by anyone. Both questions are answered here
//! from bundled data instead.
//!
//! Who sells it comes from `pricing.json`. A registrar publishes a price for
//! what it can sell you and leaves out what it cannot, so a quote is evidence
//! and the absence of one is the honest "we do not know".
//!
//! Who may buy it comes from `eligibility.json`, which is hand-maintained —
//! see the note at the top of that file.

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::pricing::Prices;
use crate::tlds::{lookup_suffix, normalize_tld};

const ELIGIBILITY_JSON: &str = include_str!("../eligibility.json");

/// Search pages for the registrars that appear in the price table.
///
/// A registrar with no entry here is still reported by name — the published
/// price is the claim, the link is only a shortcut — so adding a registrar to
/// `pricing.json` never depends on touching this list.
const SEARCH_PAGES: &[(&str, &str)] = &[
    (
        "namecheap.com",
        "https://www.namecheap.com/domains/registration/results/?domain={domain}",
    ),
    (
        "porkbun.com",
        "https://porkbun.com/checkout/search?q={domain}",
    ),
    (
        "dynadot.com",
        "https://www.dynadot.com/domain/search?domain={domain}",
    ),
    (
        "namesilo.com",
        "https://www.namesilo.com/domain/search-domains?query={domain}",
    ),
];

/// A registrar that demonstrably sells this TLD, and its price for it.
#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub registrar: String,
    /// First-year registration price, USD, as published by that registrar.
    pub price: f64,
    /// Its search page with the name filled in, where one is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
}

impl Listing {
    /// `$14.98`, matching the price column.
    pub fn price_label(&self) -> String {
        format!("${:.2}", self.price)
    }
}

/// Why a TLD is not open to everyone.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Eligibility {
    /// What kind of gate exists, in a line.
    pub note: String,
    /// The registry page the note was taken from, which is the authority.
    pub source: String,
}

#[derive(Debug, Deserialize)]
struct RawRules {
    tlds: HashMap<String, Eligibility>,
}

/// Registration restrictions, by TLD.
pub struct Rules {
    by_tld: HashMap<String, Eligibility>,
}

impl Rules {
    pub fn load() -> Result<Self> {
        let raw: RawRules = serde_json::from_str(ELIGIBILITY_JSON)?;
        Ok(Self {
            by_tld: raw
                .tlds
                .into_iter()
                .map(|(tld, rule)| (normalize_tld(&tld), rule))
                .collect(),
        })
    }

    /// Longest-suffix lookup, as everywhere else: `com.au` inherits `.au`'s
    /// Australian presence rule.
    pub fn lookup(&self, tld: &str) -> Option<&Eligibility> {
        lookup_suffix(tld, |s| self.by_tld.get(s))
    }

    #[cfg(test)]
    fn all(&self) -> impl Iterator<Item = (&String, &Eligibility)> {
        self.by_tld.iter()
    }
}

/// The registrars known to sell `tld`, cheapest first, each with the given
/// domain filled into its search page.
pub fn listings(prices: &Prices, tld: &str, domain: &str) -> Vec<Listing> {
    prices
        .offers(tld)
        .iter()
        .map(|offer| Listing {
            search: SEARCH_PAGES
                .iter()
                .find(|(host, _)| *host == offer.registrar)
                .map(|(_, page)| page.replace("{domain}", domain)),
            registrar: offer.registrar.clone(),
            price: offer.register,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prices(json: &str) -> Prices {
        let path = std::env::temp_dir().join(format!(
            "ds-registration-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, json).unwrap();
        let p = Prices::from_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        p
    }

    #[test]
    fn lists_only_registrars_that_quote_the_tld() {
        let p = prices(
            r#"{"de": [{"register": "namecheap.com", "prices": {"regular": 6.98}},
                       {"register": "porkbun.com", "prices": {"regular": 2.90}}],
                "fr": [{"register": "namecheap.com", "prices": {"regular": 17.98}}]}"#,
        );

        let de = listings(&p, "de", "brand.de");
        assert_eq!(
            de.iter().map(|l| l.registrar.as_str()).collect::<Vec<_>>(),
            ["porkbun.com", "namecheap.com"],
            "cheapest first"
        );
        assert_eq!(de[0].price_label(), "$2.90");
        assert_eq!(
            de[0].search.as_deref(),
            Some("https://porkbun.com/checkout/search?q=brand.de")
        );

        // Porkbun does not sell .fr, and no link is invented for it.
        let fr = listings(&p, "fr", "brand.fr");
        assert_eq!(fr.len(), 1);
        assert_eq!(fr[0].registrar, "namecheap.com");
    }

    #[test]
    fn a_tld_nobody_prices_lists_nobody() {
        let p = prices(r#"{"com": [{"register": "namecheap.com", "prices": {"regular": 14.98}}]}"#);
        assert!(listings(&p, "edu", "school.edu").is_empty());
    }

    #[test]
    fn an_unknown_registrar_is_named_without_a_link() {
        let p = prices(r#"{"com": [{"register": "Corp.Example", "prices": {"regular": 9.0}}]}"#);
        let com = listings(&p, "com", "brand.com");
        assert_eq!(com[0].registrar, "corp.example");
        assert!(com[0].search.is_none());
    }

    #[test]
    fn an_anonymous_quote_is_not_evidence() {
        // It still sets the price column, but it says nothing about where to buy.
        let p = prices(r#"{"com": [{"prices": {"regular": 9.0}}]}"#);
        assert_eq!(p.lookup("com").unwrap().register, 9.0);
        assert!(listings(&p, "com", "brand.com").is_empty());
    }

    #[test]
    fn bundled_rules_load_and_cite_a_source() {
        let rules = Rules::load().unwrap();

        let fr = rules.lookup("fr").unwrap();
        assert!(fr.note.contains("EU"));

        for tld in ["eu", "ca", "au", "us", "gov", "edu"] {
            assert!(rules.lookup(tld).is_some(), ".{tld}");
        }

        // A note without somewhere to check it is exactly the kind of claim
        // this file is not allowed to make.
        let mut seen = 0;
        for (tld, rule) in rules.all() {
            assert!(!rule.note.trim().is_empty(), ".{tld} has no note");
            assert!(
                rule.source.starts_with("https://"),
                ".{tld}: {}",
                rule.source
            );
            seen += 1;
        }
        assert!(seen >= 10, "{seen} rules");
    }

    #[test]
    fn sub_zones_inherit_the_zone_above() {
        let rules = Rules::load().unwrap();
        let au = rules.lookup("au").unwrap();
        assert_eq!(rules.lookup("com.au").unwrap().note, au.note);
    }

    #[test]
    fn an_open_tld_has_no_rule() {
        let rules = Rules::load().unwrap();
        // .de reads as restricted but is not: DENIC registers holders anywhere.
        for tld in ["com", "net", "io", "de", "nosuchtld12345"] {
            assert!(rules.lookup(tld).is_none(), ".{tld}");
        }
    }
}
