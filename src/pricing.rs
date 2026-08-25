//! Registration prices per TLD, from the bundled `pricing.json`.
//!
//! The file lists one entry per registrar per TLD, so a TLD's price is the
//! mean over the registrars that quote one. Amounts are USD, as published by
//! the registrars; they are a snapshot, not a quote.
//!
//! One source quotes in its own currency rather than USD: get.bd, which is the
//! only place the `.bd` family is priced at all. Those offers are converted at
//! harvest time and carry a `quoted` block — the original figure, the rate and
//! its date — beside the USD `prices`. Nothing here reads it; it is there so a
//! reader of the file can tell a derived number from a quoted one, and so a
//! re-harvest converts the source figure again rather than a converted one.
//!
//! `regular` is a registrar's published *first-year* list price, which is
//! routinely well below what the name costs to keep — `.site` is a couple of
//! dollars to register and forty-odd to renew. That is the shelf price, not a
//! coupon: `scripts/harvest-prices.mjs` takes standing list prices only and
//! drops discount codes. `renew` is carried beside it so the gap is visible.
//!
//! Like `whois.json`, the file is embedded at compile time so the binary stays
//! self-contained; a custom table in the same format can be loaded over it.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::tlds::{lookup_suffix, normalize_tld};

const PRICING_JSON: &str = include_str!("../pricing.json");

#[derive(Debug, Deserialize)]
struct RawOffer {
    /// Which registrar published this quote. Optional, because a hand-written
    /// table may not care to say — but naming it is what lets `--where` treat
    /// the quote as proof that this registrar sells the TLD.
    #[serde(default)]
    register: Option<String>,
    /// Every price key is optional so a hand-written table can quote only the
    /// figure it cares about.
    #[serde(default)]
    prices: RawPrices,
}

#[derive(Debug, Default, Deserialize)]
struct RawPrices {
    /// Published list price for the first year — not the yearly cost, which
    /// is `renew`. Null where the registrar does not sell the TLD directly.
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

/// One named registrar's first-year price for a TLD.
///
/// This is the only hard evidence `ds` has about who sells what: a registrar
/// publishes a price for a TLD it can actually sell you, and simply leaves out
/// the ones it cannot. `--where` lists these rather than guessing.
#[derive(Debug, Clone, Serialize)]
pub struct Offer {
    pub registrar: String,
    pub register: f64,
}

/// What the table knows about one TLD: the averaged price shown in the column,
/// and the individual quotes it was averaged from.
struct TldPrices {
    price: Price,
    offers: Vec<Offer>,
}

pub struct Prices {
    by_tld: HashMap<String, TldPrices>,
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
            if let Some(entry) = summarise(&offers) {
                by_tld.insert(tld, entry);
            }
        }

        Ok(Self { by_tld })
    }

    /// Longest-suffix lookup, as for WHOIS servers: `co.uk` wins over `uk`.
    pub fn lookup(&self, tld: &str) -> Option<Price> {
        self.find(tld).map(|entry| entry.price)
    }

    /// The registrars that quote a price for this TLD, cheapest first — the
    /// evidence behind `--where`. Empty when the table prices the TLD without
    /// saying who quoted it, and when nobody prices it at all.
    pub fn offers(&self, tld: &str) -> &[Offer] {
        self.find(tld).map_or(&[], |entry| &entry.offers)
    }

    fn find(&self, tld: &str) -> Option<&TldPrices> {
        lookup_suffix(tld, |s| self.by_tld.get(s))
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

/// Mean over the registrars that quote a price, plus the quotes themselves; a
/// TLD nobody sells has neither.
fn summarise(offers: &[RawOffer]) -> Option<TldPrices> {
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

    // An anonymous quote still moves the average — it is a real price — but it
    // names no registrar, so it is no evidence about where to buy the name.
    let mut named: Vec<Offer> = offers
        .iter()
        .filter_map(|o| {
            let registrar = o.register.as_deref()?.trim();
            let register = quoted(o.prices.regular)?;
            (!registrar.is_empty()).then(|| Offer {
                registrar: registrar.to_ascii_lowercase(),
                register,
            })
        })
        .collect();
    named.sort_by(|a, b| {
        a.register
            .total_cmp(&b.register)
            .then_with(|| a.registrar.cmp(&b.registrar))
    });

    Some(TldPrices {
        price: Price {
            register: mean(&register)?,
            renew: mean(&renew),
            currency: "USD",
            registrars: register.len(),
        },
        offers: named,
    })
}

/// Rounded to cents: the mean of three prices is still money, and `--json`
/// should not report a renewal of 19.279999999999998.
fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    Some((mean * 100.0).round() / 100.0)
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

    /// The table is a mean over registrars, so it has to actually hold more
    /// than one of them — a single-registrar file would make `registrars: n`
    /// and the word "average" a lie.
    #[test]
    fn bundled_table_quotes_several_registrars() {
        let p = Prices::load().unwrap();
        let multi = p.by_tld.values().filter(|v| v.price.registrars > 1).count();
        assert!(
            multi > 400,
            "only {multi} TLDs have more than one registrar quoting them"
        );
        assert!(p.lookup("com").unwrap().registrars > 1);
    }

    /// A converted offer carries a `quoted` block recording what the source
    /// actually published; nothing here reads it, and nothing here may choke
    /// on it either.
    #[test]
    fn extra_provenance_fields_are_ignored() {
        let p = Prices::parse(
            r#"{"com.bd": [{"register": "get.bd", "prices": {"regular": 6.57, "renew": 15.03},
                            "quoted": {"currency": "BDT", "regular": 805, "renew": 1840,
                                       "rate": 122.453121, "as_of": "2026-08-25",
                                       "source": "https://get.bd/pricing.php"}}]}"#,
        )
        .unwrap();

        let bd = p.lookup("com.bd").unwrap();
        assert_eq!(bd.register, 6.57);
        assert_eq!(bd.currency, "USD", "the table reports the converted figure");
        assert_eq!(p.offers("com.bd")[0].registrar, "get.bd");
    }

    /// The second-level .bd zones are what a Bangladeshi actually registers,
    /// and until get.bd was harvested none of them had a price at all — only
    /// bare `.bd` did, from one foreign reseller.
    #[test]
    fn bundled_table_prices_the_bd_family() {
        let p = Prices::load().unwrap();
        for tld in [
            "bd",
            "com.bd",
            "net.bd",
            "org.bd",
            "edu.bd",
            "co.bd",
            "xn--54b7fta0cc",
        ] {
            let price = p
                .lookup(tld)
                .unwrap_or_else(|| panic!(".{tld} should be priced"));
            assert!(price.register > 0.0, ".{tld} priced at {}", price.register);
            // A second-level zone must have its own entry rather than
            // inheriting `.bd`'s, which is a different price entirely.
            assert!(
                p.offers(tld).iter().any(|o| o.registrar == "get.bd"),
                ".{tld} should carry the registry-set quote"
            );
        }
    }

    /// The point of harvesting more registrars: TLDs one of them does not
    /// sell now have a price from one that does.
    #[test]
    fn bundled_table_reaches_past_one_registrars_catalogue() {
        let p = Prices::load().unwrap();
        for tld in ["cn", "nu"] {
            assert!(p.lookup(tld).is_some(), ".{tld} should be priced");
        }
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

    /// The two means are taken over whoever quoted that particular figure, so
    /// a registrar that only publishes a renewal still counts towards `renew`
    /// without inflating `registrars`, and one that only publishes a
    /// registration price leaves `renew` to the others.
    #[test]
    fn each_mean_counts_only_the_registrars_that_quoted_it() {
        let p = Prices::parse(
            r#"{"com": [{"register": "a.example", "prices": {"regular": 10.0}},
                        {"register": "b.example", "prices": {"regular": 30.0, "renew": 40.0}},
                        {"register": "c.example", "prices": {"renew": 60.0}}]}"#,
        )
        .unwrap();

        let com = p.lookup("com").unwrap();
        assert_eq!(com.register, 20.0);
        assert_eq!(com.registrars, 2, "c.example quoted no registration price");
        assert_eq!(com.renew, Some(50.0), "but it does count towards renewal");
    }

    /// Three registrars is the shape the bundled table is now in; the mean
    /// must not be thrown off by the order they appear in.
    #[test]
    fn averages_are_order_independent() {
        let offers = |a: &str, b: &str, c: &str| {
            format!(
                r#"{{"com": [{{"register": "{a}.example", "prices": {{"regular": 11.0, "renew": 12.0}}}},
                            {{"register": "{b}.example", "prices": {{"regular": 14.0, "renew": 18.0}}}},
                            {{"register": "{c}.example", "prices": {{"regular": 20.0, "renew": 30.0}}}}]}}"#
            )
        };
        let one = Prices::parse(&offers("a", "b", "c")).unwrap();
        let other = Prices::parse(&offers("c", "a", "b")).unwrap();

        assert_eq!(one.lookup("com").unwrap().register, 15.0);
        assert_eq!(one.lookup("com").unwrap().label(), "$15.00");
        assert_eq!(
            one.lookup("com").unwrap().renew,
            other.lookup("com").unwrap().renew
        );
        assert_eq!(one.lookup("com").unwrap().registrars, 3);
    }

    /// Thirds of a dollar do not divide evenly, and `--json` should not say
    /// so: the mean is money and comes back rounded to cents.
    #[test]
    fn means_are_rounded_to_cents() {
        let p = Prices::parse(
            r#"{"com": [{"register": "a.example", "prices": {"regular": 14.99, "renew": 19.99}},
                        {"register": "b.example", "prices": {"regular": 14.98, "renew": 18.48}},
                        {"register": "c.example", "prices": {"regular": 11.08, "renew": 11.08}}]}"#,
        )
        .unwrap();

        let com = p.lookup("com").unwrap();
        assert_eq!(com.register, 13.68);
        assert_eq!(com.renew, Some(16.52));
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
