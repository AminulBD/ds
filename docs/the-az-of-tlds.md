# The A–Z of TLDs

`tld-facts.json` is every TLD IANA records — **1,595** of them, 1,438 still in
the root zone — with what kind it is, who runs it, what it is for, and how to
reach the registry:

```json
"museum": {
  "type": "sponsored",
  "sponsor": "Museum Domain Management Association",
  "registry": "Museum Domain Management Association (MuseDoma)",
  "delegated": "2001-11-01",
  "in_root_zone": true,
  "categories": ["sponsored", "legacy-gtld", "third-level"],
  "topics": ["arts"],
  "delegation": {
    "manager": "Museum Domain Management Association",
    "country": "United States of America (the)",
    "url": "http://about.museum",
    "whois": "whois.nic.museum",
    "nameservers": ["a0.nic.museum", "a2.nic.museum", "b0.nic.museum"],
    "registered": "2001-11-01",
    "updated": "2025-06-30"
  }
}
```

## Facts and readings are different fields

`categories` is **derived, and says from what.** The file's `categories` block
states each rule once, and the fact it reads sits on the record beside it —
`new-gtld` because `delegated` is on or after 2013-10-23, `brand` because ICANN
recorded Specification 13, `country-code` because IANA types it so.

| Category | TLDs | | Category | TLDs |
| --- | ---: | --- | --- | ---: |
| `generic` | 1250 | | `unassigned` | 157 |
| `new-gtld` | 1243 | | `removed-from-root` | 157 |
| `brand` | 369 | | `contract-terminated` | 144 |
| `country-code` | 316 | | `removed` | 137 |
| `idn` | 170 | | `legacy-gtld` | 18 |
| `sponsored` | 14 | | `test` | 11 |
| `generic-restricted` | 3 | | `third-level` | 3 |
| `infrastructure` | 1 | | | |

`topics` is **not derived, and does not pretend to be.** Nothing in the root
zone says `.pizza` is about food — it records who runs a TLD, not what it
means. So the subject taxonomy lives in
[`tld-categories.json`](../tld-categories.json), hand-maintained in this repo like
`eligibility.json`, and it is our reading. **25 subjects covering 600 of the
761** TLDs that could sensibly have one:

| | | | |
| --- | --- | --- | --- |
| Places | Business & Work | Money & Finance | Internet & Technology |
| Shopping & Deals | Food & Drink | Health & Wellbeing | Sport |
| Games & Betting | Arts & Entertainment | News & Publishing | Law |
| Education & Science | Property | Travel & Hospitality | Cars & Vehicles |
| Home & Trades | Fashion & Style | Design & Creative | Faith |
| People & Community | Government & Politics | Environment & Energy | Animals |
| Adult | | | |

Three rules keep it honest. A TLD is listed **only where the name plainly says
what it is for** — `.best`, `.now`, `.xyz`, `.zip` and 160 others are left out,
because an unclassified TLD is a visible gap while a guessed one is noise that
reads like a fact. **Brand TLDs are excluded wholesale**: `.apple` is not a
fruit TLD, and its subject is its owner, which `categories` already records.
And a TLD may sit in **more than one** subject — `.kitchen` is food and home
both, which is truer than picking one.

Because nobody generates that file, `scripts/test-tld-facts-parse.mjs` checks
every TLD it names is real, still delegated, not a brand, and keyed in
punycode. A typo there would otherwise sit in the repo looking exactly like
data.

## Retired TLDs are kept, and marked

A TLD IANA still records but that is no longer in the root zone stays in the
table as `removed-from-root` — `.abarth` resolved once and does not now, which
an A–Z should say rather than silently omit. `in_root_zone` separates the live
set from the history.

## Where it comes from

Three public-domain requests, none behind a challenge:

| Source | Gives |
| --- | --- |
| [IANA root zone file](https://data.iana.org/TLD/tlds-alpha-by-domain.txt) | the authoritative set, and the serial it was read at |
| [IANA Root Zone Database](https://www.iana.org/domains/root/db) | type and sponsoring organisation, one row per TLD |
| [ICANN gTLD registry](https://www.icann.org/resources/registries/gtlds/v2/gtlds.json) | registry operator, delegation date, Specification 13, contract status |

`--deep` adds a fourth: each TLD's own **delegation record** at
`iana.org/domains/root/db/<tld>.html`, which is where the registry's manager,
country, registration URL, WHOIS and RDAP servers, name servers and dates
actually live. That is ~1,600 pages rather than three, so it is off by default,
runs **one request at a time** `--gap` seconds apart with no concurrency at any
setting, and caches every page under `scripts/.iana-cache/` so a re-run costs
nothing. IANA's `robots.txt` is `Disallow:` with an empty value — everything
permitted — and nothing there is challenged; the pacing is courtesy.

**The contacts on those pages are deliberately not collected.** Each record
also carries an administrative and a technical contact: a name, an email
address and a telephone number, often a real person's for a ccTLD. Compiling
that out of 1,600 pages into a public repository is not something a domain
search needs to do. The organisation and its country are kept; the people are
not.

```sh
node scripts/harvest-tld-facts.mjs --dry-run   # fetch and report, write nothing
node scripts/harvest-tld-facts.mjs             # rebuild tld-facts.json (3 requests)
node scripts/harvest-tld-facts.mjs --deep      # and the delegation records (~1,600, ~90 min)
node scripts/test-tld-facts-parse.mjs          # both parsers and the taxonomy, offline
```

Every TLD's own page on the site carries all of it — the registry, its country
and site, the servers it tells IANA it runs, its name servers and delegation
dates, and both kinds of category. Where the site's copy of a WHOIS host and
IANA's disagree, the page says so rather than printing two and leaving you to
spot it.

Both IANA sources are web pages, which is the brittle part — the harvest
refuses to write if it parses fewer than half the TLDs the root zone lists, a
delegation record that will not load costs that one TLD its extra fields rather
than failing the run, and the tests pin the row and record shapes they depend
on.

