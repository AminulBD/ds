# dc — domain checker

A small Rust CLI that checks domain availability across many TLDs at once.

It asks **RDAP** first (server list from the IANA bootstrap, cached locally) and
falls back to **WHOIS** for TLDs with no RDAP service, using the server and
"available" needle table in the bundled `dist.whois.json`. When a bundled WHOIS
host is stale or missing, IANA is asked who serves that TLD today.

## Build

```sh
cargo build --release
cp target/release/dc ~/.local/bin/dc     # or anywhere on your PATH
```

`dist.whois.json` is embedded into the binary at compile time, so `dc` runs
from anywhere.

## Usage

```sh
dc apple --tld all                 # every known TLD (~1650 lookups)
dc apple --tld com,net             # a specific list
dc apple --tld popular             # a curated set of ~38 common TLDs
dc apple --tld rdap                # only TLDs that have an RDAP service
dc apple --tld @tlds.txt           # one TLD per line, `,` and `#` comments ok
dc apple.com                       # a full domain, checked as-is
```

### Several names at once

Names can be comma separated, given as separate arguments, or read from a file
— every name is checked against every TLD:

```sh
dc apple,orange,bangla,english --tld com,net    # 4 names x 2 TLDs = 8 lookups
dc apple orange bangla --tld io
dc @names.txt --tld com,net,io --available-only
```

`names.txt` takes one name per line; commas work there too, `#` starts a
comment, blank lines are ignored, and duplicates are dropped:

```
apple
orange, bangla     # both checked
english
```

Results print as they arrive and are written to the output directory:

```
+ zqxwvu7391aminul.dev             AVAILABLE  rdap      415ms
- apple.com                        TAKEN      rdap      978ms
? google.pt                        UNKNOWN    -        3548ms  whois: connecting to whois.dns.pt:43: timed out

summary: 1 available  1 taken  1 unknown   (3 checked in 3.5s)
wrote: ./available.txt (1 entry)
wrote: ./unavailable.txt (1 entry)
wrote: ./unknown.txt (1 entry)
```

* `available.txt` — one domain per line
* `unavailable.txt` — registered domains
* `unknown.txt` — only written when a registry could not be reached, so a
  failed lookup is never silently filed as "available"

## Information flags

| Flag | Shows |
| --- | --- |
| `--details` | registrar, IANA ID, created/updated/expiry dates, status codes, nameservers, DNSSEC, abuse contact |
| `--registry` | which RDAP endpoint and/or WHOIS server answered |
| `--whois` | queries WHOIS as well and prints the raw record |
| `--dns-records` | A, AAAA, NS, MX, TXT, CNAME and SOA records |
| `--raw` | raw RDAP JSON |
| `--all-info` | `--details --registry --whois --dns-records` |

```sh
dc apple --tld com --details --registry --dns-records
```

```
- apple.com                        TAKEN      rdap      978ms
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

## Other options

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c, --concurrency <N>` | 20 | parallel lookups |
| `--per-host <N>` | 4 | parallel lookups against a single registry server |
| `--timeout <SECS>` | 10 | per-request timeout |
| `-o, --out-dir <DIR>` | `.` | where the `.txt` files go |
| `--no-save` | | do not write files |
| `--append` | | append instead of overwriting |
| `--available-only` | | print only available domains (files still complete) |
| `--json` | | print a JSON array instead of the text report |
| `-q, --quiet` | | summary only |
| `--whois-only` / `--no-whois` | | restrict to one protocol |
| `--refresh` | | re-download the IANA RDAP bootstrap (cached 7 days in `~/.cache/dc/`) |
| `--no-color` | | plain output (also honours `NO_COLOR`) |

Exit code: `0` if at least one domain is available, `1` if none are, `2` on a
startup error.

## Accuracy notes

* RDAP is authoritative: `404` means the name is not registered.
* WHOIS is text matching. The per-registry needle from `dist.whois.json` is
  tried first, then generic markers. Anything unrecognised is reported as
  `UNKNOWN` rather than guessed.
* Some registries answer nobody: `.li` and `.qa` refuse public WHOIS and have no
  RDAP, and a few bundled WHOIS hosts no longer exist. Those come back
  `UNKNOWN` with the reason attached.
* Three registry quirks are handled explicitly, because each one otherwise
  reads as a registration: answers that echo the queried name back before
  saying "no object found" (`.sr`, `.fj`), answers for a TLD the server does
  not serve (`.tattoo`, `.photo`), and answers that describe the *parent* zone
  instead of the name asked about (`foo.ernet.in` → `ernet.in`).
* Large sweeps get rate-limited. `dc` paces itself per host, backs off on
  403/429, and stops querying a server that has refused it six times in a row
  (retrying it after 30s). Identity Digital runs ~250 gTLDs behind one RDAP
  endpoint with a strict quota, so a single `--tld all` sweep will leave some of
  those unresolved; re-run just those later:

  ```sh
  dc apple --tld "$(sed 's/^apple\.//' unknown.txt | paste -sd, -)"
  ```

## Tests

```sh
cargo test
```
