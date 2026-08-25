# Prices

The price column is the average first-year registration price for the TLD, in
**USD**, from the bundled `pricing.json` — 881 TLDs, from `$3.76` to `$7138.99`.
The file lists one entry per registrar per TLD, so where several registrars
quote a price the column is the mean of them:

```json
{
  "com": [
    { "register": "101domain.com", "prices": { "regular": 14.99, "renew": 19.99, "transfer": 13.99 } },
    { "register": "namecheap.com", "prices": { "regular": 14.98, "renew": 18.48, "transfer": 14.98 } },
    { "register": "porkbun.com",   "prices": { "regular": 11.08, "renew": 11.08, "transfer": 11.08 } }
  ]
}
```

`register` names the registrar that published the quote, which is also what
`--where` reads to say who actually sells a TLD — so keep it accurate if you
edit the file.

Multi-label suffixes are looked up longest-first, as for WHOIS servers, so
`.co.uk` gets its own price rather than `.uk`'s. A TLD none of them price shows
`-`, which is still most of the root: `ds` knows of far more TLDs than anyone
publishes a retail price for.

Treat these as list prices, not quotes. They are a snapshot of published
registrar pricing: `regular` is the **first year**, which is routinely a
fraction of what the name then costs to keep — `.site` is a couple of dollars
to register and forty-odd to renew — so read it next to `renew` rather than as
a yearly cost. ICANN fees, taxes, premium names and time-limited promotions all
move the real number again, and a registry may not sell the TLD to you at all
(see [Where to register](showing-more.md#where-to-register)). `--json` carries the renewal
price, the currency and how many registrars went into the mean:

```json
"price": { "register": 13.68, "renew": 16.52, "currency": "USD", "registrars": 3 }
```

Prices you have actually been quoted beat a bundled snapshot — see
[Your own prices](choosing-the-source.md#your-own-prices) for how to supply them.

## Where the prices come from

`pricing.json` is harvested from registrars, not registries. Registries sell
wholesale to accredited registrars under contract and, outside the ICANN gTLD
agreements whose fees are per-registry PDFs rather than a feed, they publish no
retail price at all — there is no price a registry charges *you*. So the table
is the mean over the registrars that will actually sell you the name:

| Source | TLDs | How |
| --- | --- | --- |
| [101domain](https://www.101domain.com/pricing.htm) | 754 | one page per TLD, enumerated from their sitemap |
| [Porkbun](https://api.porkbun.com/api/json/v3/pricing/get) | 634 | public JSON pricing endpoint, no key |
| [Namecheap](https://www.namecheap.com/domains/full-domain-pricing-list/) | 569 | earlier manual snapshot; their list page and API are both closed to scripts |
| [get.bd](https://get.bd/pricing.php) | 12 | the `.bd` family, at BTCL's own list prices; the only source that prices it |

Coupon codes and banner promotions are not scraped — only standing shelf prices.

Three of those four sources quote USD and are taken as they come. get.bd quotes
Bangladeshi taka, and is converted, because the alternative was that `ds` priced
`.bd` off a single foreign reseller at eight times the registry's own list price
and priced `.com.bd` — the zone a Bangladeshi actually registers — not at all.

The conversion is not silent. A converted offer keeps what the source published
beside the USD figure, so a derived number is always tellable from a quoted one:

```json
{
  "com.bd": [
    {
      "register": "get.bd",
      "prices": { "regular": 6.57, "renew": 15.03 },
      "quoted": { "currency": "BDT", "regular": 805, "renew": 1840,
                  "rate": 122.453121, "as_of": "2026-08-25",
                  "source": "https://get.bd/pricing.php" }
    }
  ]
}
```

The rate is fetched per run from a named source and the harvest fails rather
than fall back to a stale constant, but a converted price still drifts between
harvests in a way a USD one does not — `quoted.as_of` is what says how far.

`scripts/harvest-prices.mjs` rebuilds the file, and documents each source and
its limits at the top:

```sh
node scripts/harvest-prices.mjs --dry-run          # fetch and report, write nothing
node scripts/harvest-prices.mjs --sources porkbun  # one source (a single request)
node scripts/harvest-prices.mjs                    # all of them; the crawl takes ~20 min
```

