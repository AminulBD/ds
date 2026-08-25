# ds — domain search

A small Rust CLI that checks domain availability across many TLDs at once.

```console
$ ds mybrand --tld com,net,io,de,co.uk
+ mybrand.io                       AVAILABLE    $65.98 whois     512ms
+ mybrand.de                       AVAILABLE     $6.98 whois     331ms
- mybrand.com                      TAKEN        $14.98 rdap      504ms
- mybrand.net                      TAKEN        $14.98 rdap      503ms
- mybrand.co.uk                    TAKEN         $7.48 rdap      869ms

summary: 2 available  3 taken  0 unknown   (5 checked in 1.1s)
```

Every lookup asks **RDAP** first, using the server list from the IANA bootstrap
(cached locally for a week). TLDs with no RDAP service fall back to **WHOIS** on
port 43, using the server and "available" needle table in the bundled
`whois.json`. When a bundled WHOIS host is stale or missing, IANA is asked which
server serves that TLD today.

A lookup that cannot be answered is reported as `UNKNOWN` with the reason
attached — never guessed as available.

The column after the status is what the TLD costs to register for a year, in
US dollars — see [Prices](#prices).

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

`whois.json` and `pricing.json` are embedded at compile time, so the binary runs
from anywhere.

## Usage

```sh
ds apple --tld com,net             # a specific list
ds apple --tld all                 # every known TLD (~1650 lookups)
ds apple --tld popular             # a curated set of ~38 common TLDs
ds apple --tld rdap                # only TLDs that have an RDAP service
ds apple --tld @tlds.txt           # one TLD per line, `,` and `#` comments ok
ds apple.com                       # a full domain, checked as-is
```

Results stream in as they arrive. `+` is available, `-` is taken, `?` could not
be answered:

```
+ mybrand.dev                      AVAILABLE    $15.98 rdap      415ms
- apple.com                        TAKEN        $14.98 rdap      978ms
? google.pt                        UNKNOWN           - -        3548ms  whois: connecting to whois.dns.pt:43: timed out

summary: 1 available  1 taken  1 unknown   (3 checked in 3.5s)
```

The fourth column is the average yearly registration price for the TLD; `-`
means no price is on file for it.

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
* `unknown.txt` — only when a registry could not be reached, so a failed lookup
  is never filed as "available"

With `--json` the same three files are written as `available.json`,
`unavailable.json` and `unknown.json`, each a JSON array of the full results
rather than a list of names:

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
| `any` (default) | everything | 1659 |
| `second` | plain TLDs — `apple.com`, `apple.de` | 1274 |
| `third` | multi-label suffixes — `apple.co.uk`, `apple.com.au` | 385 |

Handy for `--tld all`, which otherwise sweeps hundreds of restricted zones
(`gov.bd`, `ernet.in`, `edu.gt`) you cannot register under anyway. A full domain
typed out by hand is always checked as given — `--level` only filters TLD lists.

### By length

`--cctld` keeps only country-code TLDs — the two-letter ones, which is exactly
what ICANN reserves for ISO 3166-1 country codes:

```sh
ds apple --tld all --cctld                  # 505 (includes co.uk, com.au, ...)
ds apple --tld all --cctld --level second   # 140 bare ccTLDs: de, io, jp, ...
```

`--tld-len` is the general form, measured on the last label so `co.uk` counts
as 2:

| Spec | Keeps | Count under `--tld all` |
| --- | --- | --- |
| `--tld-len 2` | two-letter (same as `--cctld`) | 505 |
| `--tld-len 3` | `com`, `net`, `xyz`, ... | 236 |
| `--tld-len -3` | three characters or fewer | 741 |
| `--tld-len 4-` | four or more | 918 |

## Showing more

| Flag | Shows |
| --- | --- |
| `--details` | registrar, IANA ID, created/updated/expiry dates, status codes, nameservers, DNSSEC, abuse contact |
| `--registry` | which RDAP endpoint and/or WHOIS server answered |
| `--whois` | queries WHOIS as well and prints the raw record |
| `--dns-records` | A, AAAA, NS, MX, TXT, CNAME and SOA records |
| `--where` | for available domains: the registry, its official registration page, and registrar searches with the name filled in |
| `--raw` | raw RDAP JSON |
| `--json` | a JSON array instead of the text report |
| `--all-info` | `--details --registry --whois --dns-records --where` |

```sh
ds apple --tld com --details --registry --dns-records
```

```
- apple.com                        TAKEN        $14.98 rdap      978ms
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
ds mynewbrand --tld com,de --where
```

```
+ mynewbrand.de                    AVAILABLE     $6.98 whois     858ms
    registry     DENIC eG
    registry url http://www.denic.de/
    register at  https://porkbun.com/checkout/search?q=mynewbrand.de
                 https://www.namecheap.com/domains/registration/results/?domain=mynewbrand.de
                 https://www.dynadot.com/domain/search?domain=mynewbrand.de
                 https://www.namesilo.com/domain/search-domains?query=mynewbrand.de
```

The registry name and URL come from IANA's record for the TLD, looked up once
per TLD and shown only for domains that are actually available. For ccTLDs that
page is usually the registry's list of accredited registrars, which matters
because most ccTLDs are not sold by every registrar — `.de` and `.fr` have
residency or trustee requirements, for instance.

The four registrar links are prefilled searches, not a claim that those
registrars carry the TLD; their pages will say.

## Prices

The price column is the average first-year registration price for the TLD, in
**USD**, from the bundled `pricing.json` — 569 TLDs, from `$1.78` to `$2998.00`.
The file lists one entry per registrar per TLD, so where several registrars
quote a price the column is the mean of them:

```json
{
  "com": [
    { "register": "namecheap.com", "prices": { "regular": 14.98, "renew": 18.48, "transfer": 14.98 } }
  ]
}
```

Multi-label suffixes are looked up longest-first, as for WHOIS servers, so
`.co.uk` gets its own price rather than `.uk`'s. A TLD nobody in the table
prices — `.nu`, `.cn` and a handful of others — shows `-`.

Treat these as list prices, not quotes. They are a snapshot of published
registrar pricing: first-year promotions, ICANN fees, taxes and premium names
all move the real number, and a registry may not sell the TLD to you at all
(see [Where to register](#where-to-register)). `--json` carries the renewal
price and the currency alongside the registration one:

```json
"price": { "register": 14.98, "renew": 18.48, "currency": "USD", "registrars": 1 }
```

Prices you have actually been quoted beat a bundled snapshot — see
[Your own prices](#your-own-prices) for how to supply them.

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
reads `$9.54`. `register` is a label for your own benefit; only the prices are
read, and each of them may be left out or set to `null` for "not sold". A TLD
that ends up with no registration price at all shows `-`.

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
* Large sweeps get rate-limited. `ds` paces itself per host, backs off on
  403/429, and stops querying a server that has refused it six times in a row
  (retrying it after 30s). Identity Digital runs ~250 gTLDs behind one RDAP
  endpoint with a strict quota, so a single `--tld all` sweep will leave some of
  those unresolved; re-run just those later:

  ```sh
  ds apple --tld "$(sed 's/^apple\.//' unknown.txt | paste -sd, -)"
  ```

## Development

```sh
cargo test
cargo clippy --all-targets
man ./ds.1                 # preview the manual page
```

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
