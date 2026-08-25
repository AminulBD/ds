# Pacing

How hard a run leans on the registries it queries, and how much it prints. The
defaults are deliberately polite; a large sweep is paced further per host, as
[Accuracy notes](accuracy-notes.md) describes.

| Flag | Default | Meaning |
| --- | --- | --- |
| `-c, --concurrency <N>` | 20 | parallel lookups |
| `--per-host <N>` | 4 | parallel lookups against a single registry server |
| `--timeout <SECS>` | 10 | per-request timeout |
| `--refresh` | | re-download the IANA RDAP bootstrap (cached 7 days in `~/.cache/ds/`) |
| `-q, --quiet` | | summary only |
| `--no-color` | | plain output (also honours `NO_COLOR`) |
| `-v, --version` | | print the version and exit |

