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
[Prices](docs/prices.md).

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

Every release also ships `.deb`, `.rpm`, `.dmg`, `.msi` and plain archives for
Linux, macOS and Windows on x86_64, arm64 and 32-bit x86 — see
[Install](docs/install.md) for those, for the Windows notes, and for building
from source.

## Usage

```sh
ds apple --tld com,net             # a specific list
ds apple --tld all                 # every registrable TLD (~1330 lookups)
ds apple --tld popular             # a curated set of ~38 common TLDs
ds apple.com                       # a full domain, checked as-is
ds mybrand --tld all --save        # available.txt, unavailable.txt
```

Results stream in as they arrive. `+` is available, `-` is taken, `!` is a TLD
you cannot register in, `?` could not be answered. The exit code is `0` if
anything is available, `1` if nothing is, `2` on a startup error — so
`ds mybrand --tld com -q && echo free` works in a script.

[Usage](docs/usage.md) has the rest: several names at once, name and TLD files,
and what `--save` writes.

## Documentation

| Page | What is in it |
| --- | --- |
| [Install](docs/install.md) | Homebrew, the install script, packages, Windows, building from source |
| [Usage](docs/usage.md) | checking several names, reading TLDs from a file, saving results |
| [Choosing TLDs](docs/choosing-tlds.md) | `--level`, `--cctld`, `--tld-len`, and the brand TLDs left out of sweeps |
| [Showing more](docs/showing-more.md) | `--details`, `--registry`, `--dns-records`, and `--where` to register |
| [Prices](docs/prices.md) | what the price column means, and where the numbers are harvested from |
| [The A–Z of TLDs](docs/the-az-of-tlds.md) | `tld-facts.json`: what every TLD is, who runs it, what it is for |
| [Choosing the source](docs/choosing-the-source.md) | `--source`, and pointing `ds` at your own RDAP, WHOIS or price files |
| [Pacing](docs/pacing.md) | concurrency, timeouts and the quieter flags |
| [Accuracy notes](docs/accuracy-notes.md) | what each status does and does not claim |
| [The bundled WHOIS table](docs/the-bundled-whois-table.md) | how `whois.json` is generated and verified |
| [HTTP API](docs/http-api.md) | `ds serve`, behind a cargo feature, and how to run one safely |

The same pages are on [ds.aminul.dev](https://ds.aminul.dev/docs/usage), which
generates them from this directory, and `man ds` carries the full option
reference offline.

## Contributing

Bug reports, TLD data corrections, documentation fixes and code are all
welcome. [CONTRIBUTING.md](CONTRIBUTING.md) has the setup, what lives where,
how the bundled data files are generated, and the one rule that outranks the
others: a registered domain must never be reported `AVAILABLE`.

```sh
cargo test
cargo clippy --all-targets
```

## Licence

MIT — see [LICENSE](LICENSE).
