# Contributing to ds

Bug reports, TLD data corrections, documentation fixes and code are all
welcome. This page is what a contributor needs to know that the code does not
say by itself.

## The rule that outranks the others

**A registered domain must never be reported `AVAILABLE`.** Everything else in
`ds` is a convenience; that one answer is the product. It is why a lookup that
cannot be answered is `UNKNOWN` with the reason attached rather than a guess,
why a WHOIS needle is chosen from a vetted list rather than diffed out of two
responses, and why brand and reserved TLDs are `PRIVATE` rather than
`AVAILABLE`.

So a change that touches classification — `src/whois.rs`, `src/rdap.rs`,
`scripts/whois_classify.py`, or any needle in `whois.json` — is reviewed
against that rule first. "It works for the TLD I tried" is not enough; say what
a wrong answer would look like and why this cannot produce one.

The same instinct applies to the claims the tool makes about a TLD:
`--where` lists a registrar only where that registrar publishes a price, and
`eligibility.json` says who *may* buy a name only where a registry page says
so. Under-claiming is the correct failure. See
[Accuracy notes](docs/accuracy-notes.md).

## Getting set up

| You need | For | Notes |
| --- | --- | --- |
| Rust (stable) | the CLI itself | `cargo build`, no toolchain file, no MSRV pinned |
| Node 22+ | the data harvesters and the site | only `harvest-*.mjs`, `build-*.mjs` and `site/` |
| Python 3 | `scripts/refresh-whois.py` and its tests | standard library only, nothing to install |

```sh
cargo build                     # debug build
cargo run -- apple --tld com    # run it
cargo build --release --features serve && ./target/release/ds serve
```

The site is a separate Astro project:

```sh
cd site && npm install && npm run dev
```

## Before you open a pull request

```sh
cargo test
cargo clippy --all-targets
cargo fmt
cargo test --features serve                # the HTTP API is off by default
cargo clippy --all-targets --features serve
python3 scripts/test_whois_classify.py     # the harvest script's classifier
node scripts/test-tld-facts-parse.mjs      # the IANA root database parser, offline
man ./ds.1                                 # preview the manual page
```

All of those run offline. **CI does not run them** — the workflows in
`.github/workflows` build the site and cut releases, nothing else — so a red
test only turns up when somebody runs it. Please run them.

Behaviour that can be tested without a network should be. Every module carries
its own `#[cfg(test)]` tests: parsing, classification, TLD filtering and price
averaging are all covered that way, and a registry response that `ds` got wrong
makes an excellent fixture.

## Where things live

| Path | What it is |
| --- | --- |
| `src/main.rs` | the CLI: flags, the run loop, output and exit codes |
| `src/rdap.rs`, `src/whois.rs` | the two lookup paths, and the classifiers that read their answers |
| `src/bootstrap.rs` | the IANA RDAP bootstrap, and its week-long cache |
| `src/tlds.rs` | the TLD lists, `--level`, `--cctld`, `--tld-len` |
| `src/private.rs`, `src/pricing.rs`, `src/registration.rs` | brand TLDs, the price column, `--where` |
| `src/dns.rs`, `src/limit.rs`, `src/model.rs` | DNS records, per-host pacing, the shared result type |
| `src/serve.rs` | the HTTP API, behind the `serve` feature |
| `scripts/` | the harvesters that generate the data files, and their tests |
| `docs/` | the documentation, canonical — see below |
| `site/` | ds.aminul.dev, an Astro project generated from `docs/` and the data files |
| `ds.1` | the man page, hand-written and kept in step with `--help` |

## Data files

Four of them are generated. **Do not hand-edit those** — the next harvest would
drop your change, and the point of generating them is that every row is
traceable to a source.

| File | Rebuild with | Notes |
| --- | --- | --- |
| `whois.json` | `./scripts/refresh-whois.py all` | every server is probe-verified first; takes hours — see [The bundled WHOIS table](docs/the-bundled-whois-table.md) |
| `pricing.json` | `node scripts/harvest-prices.mjs` | ~20 min; `--dry-run` and `--sources <name>` for a cheap check |
| `private-tlds.json` | `node scripts/build-private-tlds.mjs` | one request to ICANN |
| `tld-facts.json` | `node scripts/harvest-tld-facts.mjs` | 3 requests; `--deep` adds ~1,600 and takes ~90 min |

Two are hand-maintained, because nothing publishes them in a machine-readable
form, and both are edited directly:

* **`eligibility.json`** — who may register in a restricted TLD. Every entry
  must name the registry page it was taken from. A rule with no citable source
  does not go in.
* **`tld-categories.json`** — what a TLD is *for*. Three rules keep it honest:
  a TLD is listed only where its name plainly says what it is for, brand TLDs
  are excluded wholesale, and a TLD may sit in more than one subject.
  `node scripts/test-tld-facts-parse.mjs` checks every TLD named there is real,
  still delegated, not a brand, and keyed in punycode.

When you do run a harvester, run it as a good guest. They are paced on purpose
— one request at a time for the IANA delegation records, a gap between queries
to the same WHOIS back end, no concurrency where the source did not invite it —
and they cache under `scripts/.iana-cache/` and `scripts/.whois-cache/` so a
re-run costs nothing. Please do not raise the concurrency to make a run finish
sooner, and do not commit the caches; they are gitignored.

Prefer `--dry-run` while you are working on a harvester. A data-only pull
request should say when the file was regenerated and by which script.

## Documentation

`docs/` is canonical. It ships inside every archive, and
[ds.aminul.dev](https://ds.aminul.dev/) generates its pages from it — so
`site/src/content/docs/` and `site/src/content/pages/` are build output and are
never edited by hand.

* Adding a page: create `docs/<slug>.md` with a `# Title` heading, then add the
  slug to `SIDEBAR` in [`site/scripts/build-docs.mjs`](site/scripts/build-docs.mjs)
  — that array is the sidebar and its order.
* Link between pages with plain relative links (`prices.md`,
  `choosing-the-source.md#your-own-prices`); the site rewrites them to routes.
* Link to a repo file with `../` (`../whois.json`); those become GitHub links.
* A new or changed flag belongs in three places: `--help` in `src/main.rs`, the
  matching page under `docs/`, and `ds.1`.
* `cd site && npm run gen` regenerates the site's content without building it,
  which is the quick way to check a docs change.

## Commits and pull requests

Keep one topic per pull request, and say in the description what the change is
for rather than only what it does — the reason is the part a reviewer cannot
reconstruct.

Commit subjects are written as a sentence describing the outcome, in the
imperative, without a `type:` prefix:

```
Punycode Unicode TLDs when normalizing, so .বাংলা resolves
Report brand and reserved TLDs as PRIVATE, not AVAILABLE
Price the .bd family from the registry, converting its taka
```

Explain the reasoning in the body, wrapped at about 72 columns. Leave unrelated
reformatting out of the diff, and note in the description anything you could
not verify — a registry you could not reach, a platform you could not test on
— rather than leaving a reviewer to discover it.

For a bug fix, include the command that reproduced it and what it printed
before and after.

## Reporting a bug

Open an [issue](https://github.com/AminulBD/ds/issues/new/choose). The forms
ask for what is almost always needed: the output of `ds --version`, the exact
command, what it printed, and what you expected.

For a wrong lookup result, add `--registry` (which server answered) and, for an
RDAP answer, `--raw`. That is usually enough to tell a registry quirk from a
bug in the classifier — and if the raw record is one `ds` reads wrongly, it
becomes the test fixture that fixes it.

## Cutting a release

Maintainers only. Pushing a `v*` tag builds ten targets, packages them and
publishes everything with a `SHA256SUMS` file:

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

`pricing.json` is embedded with `include_str!` and stored verbatim, so the
binary grows by whatever the file grows by — going from one registrar to three
took it from 94 KB to 299 KB, and a stripped release build from 4.18 MB to
4.38 MB. Worth a thought before a data file gets much bigger.

## Licence

By contributing you agree that your work is licensed under the
[MIT licence](LICENSE), the same terms as the rest of the project.
