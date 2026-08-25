# ds — domain search

A small Rust CLI that checks domain availability across many TLDs at once.

```console
$ ds mybrand --tld com,net,io,de,co.uk
+ mybrand.io                       AVAILABLE    $54.70 whois     512ms
+ mybrand.de                       AVAILABLE     $6.29 whois     331ms
- mybrand.com                      TAKEN        $13.68 rdap      504ms
- mybrand.net                      TAKEN        $14.83 rdap      503ms
- mybrand.co.uk                    TAKEN         $6.08 rdap      869ms

summary: 2 available  3 taken  0 unknown   (5 checked in 1.1s)
```

Every lookup asks **RDAP** first, using the server list from the IANA bootstrap
(cached locally for a week). TLDs with no RDAP service fall back to **WHOIS** on
port 43, using the server and "available" needle table in the bundled
`whois.json` — 1358 TLDs. When a bundled WHOIS host is stale or missing, IANA is
asked which server serves that TLD today.

A lookup that cannot be answered is reported as `UNKNOWN` with the reason
attached — never guessed as available.

The column after the status is what the TLD costs to register for its first
year, in US dollars, averaged over the registrars that sell it — see
[Prices](#prices).

## Install

On macOS or Linux with [Homebrew](https://brew.sh):

```sh
brew install aminulbd/tap/ds
```

That is the one to take if you have it — upgrades come with `brew upgrade`.
Without Homebrew, this installs the latest release into `~/.local/bin`, needing
no root and nothing preinstalled but `curl` and `tar`:

```sh
curl -fsSL https://raw.githubusercontent.com/aminulbd/ds/main/packaging/install.sh | sh
```

It creates the directories it needs, and says so if they turn out not to be on
your `PATH` or on the manual search path. Set `BIN_DIR` to install elsewhere,
or run it as root to install system-wide. It is worth reading before you pipe
it to a shell — drop the `| sh` and it just prints.

Every release also ships installers and plain archives for Linux, macOS and
Windows on x86_64, arm64 and 32-bit x86. Grab one from the
[releases page](../../releases):

| Platform | File | Install |
| --- | --- | --- |
| Debian, Ubuntu | `ds_<version>_amd64.deb` (also `arm64`, `i386`) | `sudo dpkg -i ds_*.deb` |
| Fedora, RHEL, openSUSE | `ds-<version>.x86_64.rpm` (also `aarch64`, `i686`) | `sudo rpm -i ds-*.rpm` |
| macOS | `ds-<version>-aarch64-apple-darwin.dmg` (also `x86_64`) | mount it, run `install.sh` |
| Windows | `ds-<version>-x86_64-pc-windows-msvc.msi` (also `aarch64`, `i686`) | double-click, then open a new terminal — see [Windows](#windows) |
| Anything else | `.tar.gz` / `.zip` | unpack and copy `ds` onto your `PATH` |

The `.deb` and `.rpm` carry the binary, the man page and the licence, and are
built from static musl binaries — they depend on nothing.

### Windows

Take `x86_64` unless the machine is an Arm one — a Snapdragon laptop or a
Surface Pro X — which wants `aarch64`. `i686` is there for 32-bit Windows only.

`ds.exe` is built against the Microsoft C runtime, so it needs the Visual C++
Redistributable — most machines already have it, and the giveaway when they do
not is `VCRUNTIME140.dll was not found` on the first run. Install the one that
matches the build you took:

| Build | Download |
| --- | --- |
| `x86_64` | [vc_redist.x64.exe](https://aka.ms/vs/17/release/vc_redist.x64.exe) |
| `aarch64` | [vc_redist.arm64.exe](https://aka.ms/vs/17/release/vc_redist.arm64.exe) |
| `i686` | [vc_redist.x86.exe](https://aka.ms/vs/17/release/vc_redist.x86.exe) |

Those are Microsoft's permanent links to the current release; the page behind
them is [Latest supported Visual C++ Redistributable downloads](https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist).

The `.msi` installs either for everyone or for you alone. Left alone it takes
the first route when it can: Windows asks for administrator rights and `ds.exe`
lands in `C:\Program Files\ds\bin`. **Advanced** on the first screen offers the
choice — installing for the current user only wants no administrator and goes to
`%LOCALAPPDATA%\Apps\ds\bin` instead. Either way the folder is appended to
`PATH`, the system one or yours, and a terminal that was already open keeps its
old `PATH`, so open a new one before typing `ds`.

The installer is not code signed, so SmartScreen greets it with "Windows
protected your PC". **More info** then **Run anyway** gets past it; if you would
rather check first, every release ships a `SHA256SUMS` file:

```powershell
Get-FileHash .\ds-<version>-x86_64-pc-windows-msvc.msi -Algorithm SHA256
```

Uninstalling is the usual Settings → Apps route, and both halves can be done
without the UI:

```powershell
msiexec /i ds-<version>-x86_64-pc-windows-msvc.msi /qn                # silent
msiexec /i ds-<version>-x86_64-pc-windows-msvc.msi /qn ALLUSERS=1     # everyone
msiexec /i ds-<version>-x86_64-pc-windows-msvc.msi /qn ALLUSERS=""    # just me
msiexec /x ds-<version>-x86_64-pc-windows-msvc.msi /qn                # remove
```

Without `ALLUSERS` it installs for everyone from an elevated prompt and for the
current user from an ordinary one.

Without administrator rights, take the `.zip` instead: unpack it and put
`ds.exe` wherever you like — the binary needs nothing beside it.

Colours work in Windows Terminal, PowerShell and `cmd.exe` from Windows 10 1809
onwards. An older console gets plain text rather than escape codes, and
`--no-color` or `NO_COLOR` turns them off anywhere.

Or build it yourself:

```sh
cargo build --release
install -m755 target/release/ds ~/.local/bin/ds
install -m644 ds.1 ~/.local/share/man/man1/ds.1     # then: man ds
```

`whois.json`, `pricing.json`, `private-tlds.json` and `eligibility.json` are
embedded at compile
time, so the binary runs from anywhere. `tld-facts.json` and
`tld-categories.json` — the A–Z of every TLD, what kind it is, what it is for
and who runs it — ship beside them but are not embedded; see
[The A–Z of TLDs](#the-az-of-tlds).

## Usage

```sh
ds apple --tld com,net             # a specific list
ds apple --tld all                 # every registrable TLD (~1330 lookups)
ds apple --tld popular             # a curated set of ~38 common TLDs
ds apple --tld rdap                # only TLDs that have an RDAP service
ds apple --tld @tlds.txt           # one TLD per line, `,` and `#` comments ok
ds apple.com                       # a full domain, checked as-is
```

Results stream in as they arrive. `+` is available, `-` is taken, `!` is a TLD
you cannot register in, `?` could not be answered:

```
+ mybrand.dev                      AVAILABLE    $14.74 rdap      415ms
- apple.com                        TAKEN        $13.68 rdap      978ms
! mybrand.aws                      PRIVATE           - rdap      312ms  .aws is a brand TLD — only AWS Registry LLC registers names there (ICANN Spec 13)
? google.pt                        UNKNOWN           - -        3548ms  whois: connecting to whois.dns.pt:43: timed out

summary: 1 available  1 taken  1 private  1 unknown   (4 checked in 3.5s)
```

The fourth column is the average first-year registration price for the TLD; `-`
means no registrar in the bundled table prices it.

The exit code is `0` if anything is available, `1` if nothing is, `2` on a
startup error — so `ds mybrand --tld com -q && echo free` works in a script.

### Several names at once

Comma separated, as separate arguments, or from a file. Every name is checked
against every TLD:

```sh
ds apple,orange,bangla,english --tld com,net    # 4 names x 2 TLDs = 8 lookups
ds apple orange bangla --tld io
ds @names.txt --tld com,net,io --available-only
```

`names.txt` takes one name per line; commas work there too, `#` starts a
comment, blank lines are ignored and duplicates are dropped:

```
apple
orange, bangla     # both checked
english
```

### Saving the results

Nothing is written unless you ask for it:

```sh
ds mybrand --tld all --save                 # available.txt, unavailable.txt
ds mybrand --tld all -o results             # a directory implies --save
ds mybrand --tld popular --append           # so does --append
```

* `available.txt` — one domain per line
* `unavailable.txt` — registered domains
* `private.txt` — only when a [private TLD](#brand-and-reserved-tlds) was
  checked: names that are unregistered but not for sale
* `unknown.txt` — only when a registry could not be reached, so a failed lookup
  is never filed as "available"

With `--json` the same files are written as `available.json`,
`unavailable.json`, `private.json` and `unknown.json`, each a JSON array of the
full results rather than a list of names:

```sh
ds mybrand --tld all --save --json          # available.json, unavailable.json
```

`--append` merges into the array already in the file, so the result is still
one valid JSON document.

`--available-only` trims what is printed, not what is saved.

## Choosing TLDs

### By level

`--level` filters the `--tld` list by where the name would actually sit:

| Value | Keeps | Count under `--tld all` |
| --- | --- | --- |
| `any` (default) | everything | 1329 |
| `second` | plain TLDs — `apple.com`, `apple.de` | 944 |
| `third` | multi-label suffixes — `apple.co.uk`, `apple.com.au` | 385 |

Handy for `--tld all`, which otherwise sweeps hundreds of restricted zones
(`gov.bd`, `ernet.in`, `edu.gt`) you cannot register under anyway. A full domain
typed out by hand is always checked as given — `--level` only filters TLD lists.

### By length

`--cctld` keeps only country-code TLDs — the two-letter ones, which is exactly
what ICANN reserves for ISO 3166-1 country codes:

```sh
ds apple --tld all --cctld                  # 538 (includes co.uk, com.au, ...)
ds apple --tld all --cctld --level second   # 173 bare ccTLDs: de, io, jp, ...
```

`--tld-len` is the general form, measured on the last label so `co.uk` counts
as 2:

| Spec | Keeps | Count under `--tld all` |
| --- | --- | --- |
| `--tld-len 2` | two-letter (same as `--cctld`) | 538 |
| `--tld-len 3` | `com`, `net`, `xyz`, ... | 157 |
| `--tld-len -3` | three characters or fewer | 695 |
| `--tld-len 4-` | four or more | 634 |

### Brand and reserved TLDs

Some TLDs are not on sale to anybody. `.aws`, `.google`, `.bmw` and 364 others
are **brand TLDs**: the registry operator is the only party that may hold a
name in the zone, so a lookup for `mybrand.aws` truthfully answers "no such
domain" — which reads as AVAILABLE and is worth nothing. `.arpa` and the names
the IETF reserves (`.test`, `.example`, `.invalid`, `.localhost`, `.local`,
`.onion`) are closed for the same practical reason.

`ds` reports those as `PRIVATE` rather than `AVAILABLE`, with the reason and
the operator attached:

```sh
ds mybrand --tld aws
```

```
! mybrand.aws                      PRIVATE           - rdap      312ms  .aws is a brand TLD — only AWS Registry LLC registers names there (ICANN Spec 13)
```

**They are left out of `--tld all`, `--tld rdap` and `--tld popular` by
default** — 368 of the 1697 TLDs in `all`, so a sweep is a fifth shorter and
carries no dead ends. The run says so when it happens. A TLD you name yourself
is always checked; the default only prunes the lists `ds` picks for you.

| `--private` | Effect |
| --- | --- |
| *(unset)* | excluded from `all` / `rdap` / `popular`; a TLD you name is still checked |
| `exclude` | excluded everywhere, a hand-written `--tld` list included |
| `include` | checked, and still reported `PRIVATE` |
| `only` | check nothing else — what a sweep is skipping |

```sh
ds mybrand --tld all --private include      # all 1697, brands marked PRIVATE
ds mybrand --tld all --private only         # just the 368 closed ones
```

The classification is bundled as `private-tlds.json` and comes from ICANN's
machine-readable [registry-agreement
list](https://www.icann.org/resources/registries/gtlds/v2/gtlds.json). A
registry that runs its TLD purely for itself applies for **Specification 13**,
the ".brand TLD" exemption from the registrar non-discrimination rules, and
that flag is what `ds` reads — a dated, first-party, checkable fact rather than
a hand-picked list of famous names. Regenerate it with `node
scripts/build-private-tlds.mjs`.

The converse is deliberately *not* asserted. A TLD missing from that file is
not thereby claimed to be registrable; `ds` simply makes no claim and reports
whatever the registry said. So `.app` and `.dev` — Google-run, but with no
Specification 13 and sold through ordinary registrars — are ordinary TLDs here,
and every ccTLD is left exactly as it was.

## Showing more

| Flag | Shows |
| --- | --- |
| `--details` | registrar, IANA ID, created/updated/expiry dates, status codes, nameservers, DNSSEC, abuse contact |
| `--registry` | which RDAP endpoint and/or WHOIS server answered |
| `--whois` | queries WHOIS as well and prints the raw record |
| `--dns-records` | A, AAAA, NS, MX, TXT, CNAME and SOA records |
| `--where` | for available domains: the registry, its official registration page, any eligibility rule, and the registrars whose published prices show they sell the TLD |
| `--raw` | raw RDAP JSON |
| `--json` | a JSON array instead of the text report |
| `--all-info` | `--details --registry --whois --dns-records --where` |

```sh
ds apple --tld com --details --registry --dns-records
```

```
- apple.com                        TAKEN        $13.68 rdap      978ms
    rdap: https://rdap.verisign.com/com/v1/
    registrar    Nom-iq Ltd. dba COM LAUDE
    created      1987-02-19T05:00:00Z
    expires      2027-02-20T05:00:00Z
    iana id      470
    status       client delete prohibited, client transfer prohibited, ...
    nameservers  a.ns.apple.com, b.ns.apple.com, c.ns.apple.com, d.ns.apple.com
    dnssec       unsigned
    abuse        abuse@comlaude.com
    a            17.253.144.10
    mx           10 mx-in.g.apple.com., 20 mx-in-hfd.apple.com., ...
```

### Where to register

```sh
ds mynewbrand --tld de,fr --where
```

```
+ mynewbrand.de                    AVAILABLE     $6.29 whois     858ms
    registry     DENIC eG
    registry url http://www.denic.de/
    register at  porkbun.com       $2.90  https://porkbun.com/checkout/search?q=mynewbrand.de
                 namecheap.com     $6.98  https://www.namecheap.com/domains/registration/results/?domain=mynewbrand.de
+ mynewbrand.fr                    AVAILABLE    $17.98 rdap     1179ms
    registry     Association Française pour le Nommage Internet en Coopération (A.F.N.I.C.)
    registry url https://www.nic.fr
    eligibility  EU presence: the registrant must reside in, or have its registered office in, the EU/EEA or Switzerland
                 https://www.afnic.fr/en/observatory-and-resources/documents-to-consult-or-download/naming-policies/
    register at  namecheap.com    $17.98  https://www.namecheap.com/domains/registration/results/?domain=mynewbrand.fr
```

The registry name and URL come from IANA's record for the TLD, looked up once
per TLD and shown only for domains that are actually available. For ccTLDs that
page is usually the registry's own list of accredited registrars, which is the
answer worth having when a TLD is not sold on the open market.

**`register at` is evidence, not a guess.** Each line is a registrar that
publishes a price for that TLD in [`pricing.json`](#your-own-prices), cheapest
first — nobody quotes a price for something they cannot sell you. That is why
`.fr` above lists one registrar and `.de` two: Porkbun's price list carries
`.de` and not `.fr`. A TLD no registrar in the table prices says so plainly:

```
+ mynewbrand.edu                   AVAILABLE         - whois     365ms
    eligibility  US institutions accredited by an agency the Department of Education recognises
                 https://www.educause.edu/edu-domain
    register at  no registrar in the price table sells .edu
```

**`eligibility` is who may buy it.** A name being free says nothing about
whether the registry would let you have it, so a TLD with a residency, nexus or
membership rule prints that rule and the registry page it came from *before* the
places to buy it. This comes from the bundled `eligibility.json`, which — unlike
`whois.json` and `pricing.json` — is hand-maintained, because no registry
publishes eligibility rules in a machine-readable form. Every entry names the
page it was taken from, and that page, not `ds`, is the authority. The list
covers the restricted TLDs a registrar is likely to offer you; **a TLD missing
from it is not thereby open**.

Both halves are bundled, so `--where` still answers the useful part of the
question with `--no-iana` or no route to `whois.iana.org` — only the registry
name and URL drop out.

## Prices

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
(see [Where to register](#where-to-register)). `--json` carries the renewal
price, the currency and how many registrars went into the mean:

```json
"price": { "register": 13.68, "renew": 16.52, "currency": "USD", "registrars": 3 }
```

Prices you have actually been quoted beat a bundled snapshot — see
[Your own prices](#your-own-prices) for how to supply them.

### Where the prices come from

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

## The A–Z of TLDs

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

### Facts and readings are different fields

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
[`tld-categories.json`](tld-categories.json), hand-maintained in this repo like
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

### Retired TLDs are kept, and marked

A TLD IANA still records but that is no longer in the root zone stays in the
table as `removed-from-root` — `.abarth` resolved once and does not now, which
an A–Z should say rather than silently omit. `in_root_zone` separates the live
set from the history.

### Where it comes from

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

Both IANA sources are web pages, which is the brittle part — the harvest
refuses to write if it parses fewer than half the TLDs the root zone lists, a
delegation record that will not load costs that one TLD its extra fields rather
than failing the run, and the tests pin the row and record shapes they depend
on.

## Choosing the source

By default a lookup falls through three sources: RDAP, then the bundled
`whois.json` table, then a WHOIS server that IANA says currently serves the TLD.
`--source` restricts that:

| Value | Uses |
| --- | --- |
| `auto` (default) | RDAP -> `whois.json` -> IANA referral |
| `rdap` | RDAP only. TLDs with no RDAP service return `UNKNOWN` immediately, with no network traffic |
| `whois` | `whois.json` only. No RDAP, no IANA referral — exactly what the bundled file says |

```sh
ds apple --tld com,co,de --source rdap     # .co and .de have no RDAP -> UNKNOWN
ds apple --tld com,co,de --source whois    # all three over port 43
```

`--no-iana` is the narrower switch: keep the default order but never talk to
`whois.iana.org`, so a stale bundled WHOIS host is not repaired and `--where`
shows only the registrar links.

### Your own RDAP servers

Point TLDs at servers of your choosing with an `rdap.json`, written in the same
RDAP bootstrap format IANA publishes (RFC 9224) — so you can start from a copy
of `dns.json` and edit it:

```json
{
  "version": "1.0",
  "description": "my RDAP servers",
  "services": [
    [["com"], ["https://rdap.verisign.com/com/v1/"]],
    [["io"], ["https://rdap.identitydigital.services/rdap/"]],
    [["internal"], ["https://rdap.corp.example/", "https://rdap-backup.corp.example/"]]
  ]
}
```

Each entry maps a list of TLDs to a list of servers, tried in order.

```sh
ds apple --tld com,io --rdap-file servers.json              # merged over IANA
ds apple --tld internal --rdap-file servers.json --rdap-mode only
```

| Mode | Effect |
| --- | --- |
| `merge` (default) | your entries win for the TLDs they name, everything else still comes from the IANA bootstrap |
| `only` | the IANA bootstrap is not consulted or downloaded at all |

Without `--rdap-file`, `./rdap.json` is picked up if it exists, then
`~/.config/ds/rdap.json`. A line saying which file was loaded, how many TLDs it
covers and which mode is in force is printed before the results.

This is how you reach a registry that is missing from the bootstrap, test a
staging RDAP server, or serve an internal zone that has no public entry at all.

### Your own WHOIS servers

The same thing for WHOIS, in the same format as the bundled `whois.json` — a
list of extensions, a server and the text that marks a free domain:

```json
[
  { "extensions": ".internal,.corp",
    "uri": "socket://whois.corp.example",
    "available": "not registered" },
  { "extensions": ".de",
    "uri": "socket://whois.denic.de",
    "available": "Status: free" }
]
```

`uri` is `socket://host[:port]` for classic port-43 WHOIS, or an `http(s)://`
prefix the domain is appended to. `available` is matched against the response
to decide the name is free; matching is negation-aware, so a needle of
"Available" is not triggered by "Not Available".

```sh
ds apple --tld de,ch --whois-file servers.json                 # merged
ds apple --tld internal --whois-file servers.json --whois-mode only
```

| Mode | Effect |
| --- | --- |
| `merge` (default) | your entries win for the TLDs they name, the rest of the bundled table still applies |
| `only` | the bundled table is ignored entirely |

`./whois.json` and `~/.config/ds/whois.json` are picked up automatically, and
the run says which file it loaded — exactly as for `rdap.json`.

### Your own prices

And the same again for the price column, in the format of the bundled
`pricing.json` — a TLD mapped to what each registrar charges for it:

```json
{
  "com": [
    { "register": "porkbun.com",  "prices": { "regular": 9.13, "renew": 11.06 } },
    { "register": "namesilo.com", "prices": { "regular": 9.95, "renew": 11.79 } }
  ],
  "internal": [
    { "register": "corp.example", "prices": { "regular": 0.0 } }
  ]
}
```

Where several registrars quote a TLD the column shows the mean, so `.com` above
reads `$9.54`. Each price may be left out or set to `null` for "not sold", and a
TLD that ends up with no registration price at all shows `-`.

`register` names the registrar, and `--where` reads it: a quote here is what
tells `ds` that this registrar sells that TLD, so the two registrars above are
what `ds apple --tld com --where` would then offer you. Leave it out and the
price still counts toward the column, but the entry claims nothing about where
to buy the name.

```sh
ds apple --tld com,io --pricing-file myprices.json                 # merged
ds apple --tld internal --pricing-file myprices.json --pricing-mode only
```

| Mode | Effect |
| --- | --- |
| `merge` (default) | your prices win for the TLDs they name, the rest of the bundled table still applies |
| `only` | the bundled table is ignored entirely; TLDs you did not price show `-` |

`./pricing.json` and `~/.config/ds/pricing.json` are picked up automatically,
and the run says which file it loaded — exactly as for `rdap.json`.

This is how you put your registrar's real prices in the column, price a TLD the
bundled table has never heard of, or hold a corporate zone at nothing.

## Pacing

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c, --concurrency <N>` | 20 | parallel lookups |
| `--per-host <N>` | 4 | parallel lookups against a single registry server |
| `--timeout <SECS>` | 10 | per-request timeout |
| `--refresh` | | re-download the IANA RDAP bootstrap (cached 7 days in `~/.cache/ds/`) |
| `-q, --quiet` | | summary only |
| `--no-color` | | plain output (also honours `NO_COLOR`) |
| `-v, --version` | | print the version and exit |

## Accuracy notes

* RDAP is authoritative: `404` means the name is not registered.
* "Not registered" and "you can register it" are not the same claim. In a brand
  or reserved TLD every lookup comes back unregistered forever, so those are
  reported `PRIVATE` — see [Brand and reserved
  TLDs](#brand-and-reserved-tlds). Only Specification 13 brands and
  RFC-reserved names are called that; nothing else is assumed either way.
* WHOIS is text matching. The per-registry needle from `whois.json` is tried
  first, then generic markers. Anything unrecognised is `UNKNOWN` rather than a
  guess.
* Needle matching is negation-aware. auDA's availability service answers
  `Available` or `Not Available` and the bundled needle for `.au` is
  "Available", so a plain substring test reports every taken `.au` domain as
  free. Matches preceded by "not"/"no", or attached as in "unavailable", do not
  count.
* Four registry quirks are handled explicitly, because each one otherwise reads
  as a registration: answers that echo the queried name back before saying "no
  object found" (`.sr`, `.fj`), answers for a TLD the server does not serve
  (`.tattoo`, `.photo`), non-RDAP error bodies returned with HTTP 200 (`.sn`),
  and answers describing the *parent* zone instead of the name asked about
  (`foo.ernet.in` -> `ernet.in`).
* Some registries answer nobody: `.li` and `.qa` refuse public WHOIS and have no
  RDAP, and a few bundled WHOIS hosts no longer exist. Those come back `UNKNOWN`
  with the reason attached.
* Most of the bundled table is harvested from IANA and tested against the
  registry before being written — see
  [The bundled WHOIS table](#the-bundled-whois-table). The older hand-written
  entries have not been through that mill.
* `--where` reports registrars from published prices, so it under-claims rather
  than over-claims: a registrar missing from the bundled `pricing.json` may well
  sell the TLD anyway. `eligibility.json` is the same shape of promise in the
  other direction — the restrictions it lists are sourced, but a TLD it does not
  list is not thereby unrestricted.
* Large sweeps get rate-limited. `ds` paces itself per host, backs off on
  403/429, and stops querying a server that has refused it six times in a row
  (retrying it after 30s). Identity Digital runs ~250 gTLDs behind one RDAP
  endpoint with a strict quota, so a single `--tld all` sweep will leave some of
  those unresolved; re-run just those later:

  ```sh
  ds apple --tld "$(sed 's/^apple\.//' unknown.txt | paste -sd, -)"
  ```

## The bundled WHOIS table

Most of `whois.json` is generated rather than hand-maintained, by
[`scripts/refresh-whois.py`](scripts/refresh-whois.py). It walks IANA's list of
delegated TLDs, asks `whois.iana.org` which server serves each one, and then —
the part that matters — tests every answer before believing it. For each TLD it
asks the server about a sixteen-character random label nobody can have
registered, and about a name the *DNS* proves is registered because it has NS
records. A server only reaches the table if the first reads as AVAILABLE and the
second reads as TAKEN, judged by a port of `ds`'s own classifier.

The `available` needle is then chosen from a list of registry phrasings vetted by
hand, and kept only if it is absent from the registered name's record. A needle
is never invented by diffing two responses: the wrong needle turns a registered
domain into an `AVAILABLE`, which is the one answer `ds` must never give. A TLD
whose server cannot be shown to tell the two names apart is left out of the file
rather than guessed at, and `ds` falls back to asking IANA at runtime.

`scripts/whois-report.tsv` records the verdict and the reason for every TLD
considered, so a rejection can be looked up rather than wondered about.

```sh
./scripts/refresh-whois.py all        # harvest, verify, rewrite whois.json
cargo build --release
./scripts/refresh-whois.py verify     # re-check the table through ds itself
```

The run is paced to be a good guest: queries to one registry are serialised with
a gap between them, keyed on the address the server resolves to rather than its
name, because hundreds of `whois.nic.<tld>` aliases sit on a handful of shared
back ends. Expect it to take hours.
## HTTP API

`ds` can answer the same checks over HTTP, for a web front end or anything else
that would rather call an API than shell out. It is **not** in the released
binaries or packages: the server is behind a cargo feature, so a default build
stays the dependency-light static CLI it has always been.

```sh
cargo build --release --features serve
ds serve                      # http://127.0.0.1:8080
```

| Endpoint | What it does |
| --- | --- |
| `GET /v1/check?name=…&tld=…` | checks the names and returns the same JSON `ds --json` prints |
| `GET /healthz` | `{"status":"ok","version":"…"}`, never touches a registry |

```console
$ curl -s 'http://127.0.0.1:8080/v1/check?name=apple&tld=com,net'
[
  {"domain":"apple.com","tld":"com","status":"taken","method":"rdap","elapsed_ms":511,
   "price":{"currency":"USD","register":14.98,"renew":18.48,"registrars":1}},
  {"domain":"apple.net","tld":"net","status":"taken","method":"rdap","elapsed_ms":535,
   "price":{"currency":"USD","register":14.98,"renew":18.58,"registrars":1}}
]
```

`name` and `tld` may both be repeated or comma separated, and `tld` takes the
same `popular` and `rdap` keywords `--tld` does. With no `tld`, a dotted name is
a whole domain and a bare label means `.com` — again as on the command line.
Results come back in the order asked for. Anything rejected is a 4xx with
`{"error": "…"}`; `Retry-After` comes with a 429.

`status` is `available`, `taken`, **`unknown`** or **`private`**, and both of the
statuses that are not a plain yes or no mean here exactly what they mean on the
command line. An inconclusive lookup is `unknown`, and a response holding one is
sent `Cache-Control: no-store`, since it is a lookup that failed rather than an
answer to keep. A name in a brand or reserved zone is `private`, with the reason
in `note` — a settled fact out of a bundled table, so it is as cacheable as a
`taken`. Nothing in the server turns either of them into `available`.

There is no `private=` parameter, and the rule from [Brand and reserved
TLDs](#brand-and-reserved-tlds) applies as it does to the CLI: `tld=all`,
`tld=rdap` and `tld=popular` leave the closed zones out, and a TLD the caller
names is checked whatever it is. So `tld=aws` is answered — `private`, with the
operator attached — which is what `--private include` is for on the command
line. What the API will not do is let one request spend hundreds of registry
lookups sweeping zones the bundled table already has the answer for.

### Running one

A reachable `ds` server is an open proxy onto other people's registries: every
request it accepts becomes a query made under *your* address, against a service
that rate-limits hard. It therefore ships closed — loopback only, capped, paced
and throttled — and the defaults are the interesting part:

| Flag | Default | Meaning |
| --- | --- | --- |
| `--host <ADDR>` | `127.0.0.1` | bind address; anything else prints a warning |
| `-p, --port <N>` | 8080 | port |
| `--max-lookups <N>` | 50 | most domains one request may ask about (names × TLDs) |
| `--rate-limit <N>` | 60 | requests per minute per client address; 0 turns it off |
| `--cache-ttl <SECS>` | 300 | how long an answer is reused; 0 turns the cache off |
| `-c, --concurrency <N>` | 20 | lookups in flight across the whole server |
| `--per-host <N>` | 4 | parallel lookups against a single registry server |
| `--timeout <SECS>` | 10 | per-request timeout |
| `--cors <ORIGIN>` | | send `Access-Control-Allow-Origin`, so a browser page may call it |
| `--source <auto\|rdap\|whois>` | `auto` | as the CLI's `--source` |
| `--details` `--registry` `--where` | | as the CLI's flags of the same name |
| `--no-iana` | | never query `whois.iana.org` |

The pacing described under [Pacing](#pacing) is process-wide, not per request:
one `HostLimiter`, so a hundred clients asking about `.com` queue behind the
same per-host cap and share the same back-off. `--max-lookups` is what rules
out `tld=all` — 50 leaves room for `tld=popular`.

The output flags default off, as they do on the command line. Raw WHOIS and raw
RDAP have no query parameter at all: they are bulky, they are the registry's
text to publish rather than ours to mirror, and WHOIS records carry contact
details the parsed `--details` fields leave out.

Behind a reverse proxy every client looks like the proxy, so `--rate-limit`
becomes one budget for all of them — put the per-user limit in the proxy.
`X-Forwarded-For` is deliberately not trusted: it is a header anyone can write,
and honouring it would hand every client an unlimited supply of identities.

One thing to know about the subcommand: `ds serve` is a name a domain could
have had. In a build with the feature on it starts the server, so checking the
*name* `serve` wants `ds serve.com` or `ds -- serve`. Default builds have no
subcommand and are unaffected.

## Development

```sh
cargo test
cargo clippy --all-targets
python3 scripts/test_whois_classify.py  # the harvest script's classifier
node scripts/test-tld-facts-parse.mjs  # the IANA root database parser, offline
man ./ds.1                             # preview the manual page
cargo test --features serve            # the HTTP API is off by default
cargo clippy --all-targets --features serve
man ./ds.1                 # preview the manual page

node scripts/build-private-tlds.mjs    # refresh private-tlds.json from ICANN
node scripts/harvest-prices.mjs        # rebuild pricing.json from the registrars
node scripts/harvest-tld-facts.mjs     # rebuild tld-facts.json from IANA and ICANN
node scripts/harvest-tld-facts.mjs --deep  # ...and every TLD's delegation record
```

`pricing.json` is embedded with `include_str!` and stored verbatim, so the
binary grows by whatever the file grows by — going from one registrar to three
took it from 94 KB to 299 KB, and a stripped release build from 4.18 MB to
4.38 MB.

## Releases

Pushing a `v*` tag builds ten targets, packages them and publishes everything
with a `SHA256SUMS` file:

```sh
git tag -a v0.1.5 -m "ds 0.1.5" && git push origin v0.1.5
```

| OS | Targets | Artifacts |
| --- | --- | --- |
| Linux | `x86_64`, `aarch64` (gnu + musl), `i686` (musl) | `.tar.gz`, and `.deb` + `.rpm` from the musl builds |
| macOS | `aarch64`, `x86_64` | `.tar.gz`, `.dmg` |
| Windows | `x86_64`, `aarch64`, `i686` | `.zip`, `.msi` |

`workflow_dispatch` runs the same matrix without publishing, which is the way
to check the build before tagging.

## Licence

MIT — see [LICENSE](LICENSE).
