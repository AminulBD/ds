# ds documentation

These pages are the canonical documentation for [`ds`](../README.md). They ship
inside every archive and package, and [ds.aminul.dev](https://ds.aminul.dev/)
generates its docs from this directory — so nothing here has a second copy that
could drift.

| Page | What is in it |
| --- | --- |
| [Install](install.md) | Homebrew, the install script, packages, Windows, building from source |
| [Usage](usage.md) | checking several names, reading TLDs from a file, saving results |
| [Choosing TLDs](choosing-tlds.md) | `--level`, `--cctld`, `--tld-len`, and the brand TLDs left out of sweeps |
| [Showing more](showing-more.md) | `--details`, `--registry`, `--dns-records`, and `--where` to register |
| [Prices](prices.md) | what the price column means, and where the numbers are harvested from |
| [The A–Z of TLDs](the-az-of-tlds.md) | `tld-facts.json`: what every TLD is, who runs it, what it is for |
| [Choosing the source](choosing-the-source.md) | `--source`, and pointing `ds` at your own RDAP, WHOIS or price files |
| [Pacing](pacing.md) | concurrency, timeouts and the quieter flags |
| [Accuracy notes](accuracy-notes.md) | what each status does and does not claim |
| [The bundled WHOIS table](the-bundled-whois-table.md) | how `whois.json` is generated and verified |
| [HTTP API](http-api.md) | `ds --serve`, in every build, and how to run one safely |

`man ds` carries the full option reference offline, and
[`ds.1`](../ds.1) is the same page in the repo.
