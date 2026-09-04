# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## The rule that outranks the others

**A registered domain must never be reported `AVAILABLE`.** Everything else in
`ds` is a convenience; that one answer is the product. A lookup that cannot be
answered is `UNKNOWN` with the reason attached, never a guess. Under-claiming is
the correct failure mode everywhere — `--where` names a registrar only where
that registrar publishes a price, and `eligibility.json` states who may buy a
name only where a registry page says so.

Any change to `src/whois.rs`, `src/rdap.rs`, `scripts/whois_classify.py`, or a
needle in `whois.json` is judged against that rule first. "It works for the TLD
I tried" is not enough — say what a wrong answer would look like and why the
change cannot produce one.

## Commands

```sh
cargo build                      # debug build
cargo run -- apple --tld com     # run it
cargo run -- --serve             # the HTTP API on http://127.0.0.1:8080
```

Before opening a pull request — all of these run offline:

```sh
cargo test
cargo clippy --all-targets
cargo fmt
cargo build --no-default-features          # the CLI without the HTTP API
python3 scripts/test_whois_classify.py     # the harvest script's classifier
node scripts/test-tld-facts-parse.mjs      # the IANA root database parser
man ./ds.1                                 # preview the manual page
```

A single Rust test: `cargo test <name>` (e.g. `cargo test harvest_script_classifier_is_in_sync`);
one module's tests: `cargo test whois::`. There are no integration tests — every
module carries its own `#[cfg(test)]` block, and tests must not need a network.

**CI does not run any of this.** The workflows in `.github/workflows` build the
site and cut releases, nothing else, so a red test only surfaces when somebody
runs it locally.

The site is a separate Astro project (Node 22+):

```sh
cd site && npm install && npm run dev
```

`npm run gen` regenerates the site's content without building — the quick way to
check a docs change.

## Architecture

A single Rust binary (`src/main.rs`, `mod` declarations at the top) plus an
Astro site and a set of Node/Python harvesters that generate the bundled data.

**The lookup pipeline** lives in `check()` in `src/main.rs`, and reading it is
the fastest way to understand the tool. Per domain, in order:

1. **RDAP** (`src/rdap.rs`) using the server list from the IANA bootstrap
   (`src/bootstrap.rs`, fetched from `data.iana.org` and cached for a week under
   `$XDG_CACHE_HOME/ds/rdap-dns.json`).
2. **WHOIS** (`src/whois.rs`) on port 43, only as a fallback or when the raw
   record was asked for. The bundled server from `whois.json` is tried first; if
   it is gone or says nothing useful, IANA is asked who serves that TLD today,
   and an answer from that referral wins and clears earlier notes.
3. **Registrability** (`src/private.rs`) — a brand or reserved TLD is `PRIVATE`,
   not `AVAILABLE`, because "no such domain" there is not an offer. This step
   runs last so the IANA referral path cannot clear its note.

Every path funnels into `CheckResult` in `src/model.rs`; `--json`, the plain
output in `print_result()`, and the HTTP API all serialize that one type.

**Pacing is per-host, not global** (`src/limit.rs`). Hundreds of TLDs share a
handful of servers — Identity Digital alone runs ~250 gTLDs — so a global
concurrency cap would still get a `--tld all` run 403'd. The `HostLimiter` caps
concurrency per host, widens the gap between requests when a host pushes back,
and trips a circuit breaker after 6 consecutive refusals. Do not raise
concurrency to make a run finish sooner; the same restraint applies to the
harvest scripts, which are deliberately paced and cache under
`scripts/.iana-cache/` and `scripts/.whois-cache/` (both gitignored).

**`--serve`** (`src/serve.rs`) ships in the default build behind the `serve`
Cargo feature. A `ds` server is an open proxy onto other people's registries, so
it is loopback-only by default, caps how much one request may ask for, shares a
single `HostLimiter` process-wide, rate-limits per client, and caches responses.
`--no-default-features` drops it and builds the CLI alone.

**Data files are embedded with `include_str!`** — `whois.json` (`src/tlds.rs`),
`pricing.json`, `private-tlds.json`, `eligibility.json` — so the binary is
self-contained. Each can be overridden at runtime by a file of the same name in
the working directory or `$XDG_CONFIG_HOME/ds/`; `discover_config()` in
`src/main.rs` resolves that, and a loaded override is always announced so a
stray file cannot quietly change results. Growth in these files is binary
growth: going from one registrar to three took `pricing.json` from 94 KB to
299 KB and the stripped release build from 4.18 MB to 4.38 MB.

**The Rust and Python WHOIS classifiers are kept in sync by a test.**
`scripts/refresh-whois.py` decides which harvested servers are safe to bundle by
running a Python port of `classify()`. `harvest_script_classifier_is_in_sync` in
`src/whois.rs` parses `scripts/whois_classify.py` and compares the marker
tables — edit one and you must edit the other.

## Data files

Four are generated. **Do not hand-edit them**; the next harvest drops the change,
and the point of generating them is that every row is traceable to a source.

| File | Rebuild with |
| --- | --- |
| `whois.json` | `./scripts/refresh-whois.py all` (probe-verifies every server; takes hours) |
| `pricing.json` | `node scripts/harvest-prices.mjs` (~20 min; `--dry-run`, `--sources <name>`) |
| `private-tlds.json` | `node scripts/build-private-tlds.mjs` (one ICANN request) |
| `tld-facts.json` | `node scripts/harvest-tld-facts.mjs` (3 requests; `--deep` adds ~1,600, ~90 min) |

Two are hand-maintained, because nothing publishes them machine-readably:

* `eligibility.json` — two maps. `tlds` is who may register in a restricted
  TLD; `requirements` is what the registry asks of the name afterwards, such as
  an HTTPS-only zone or a rule about what the name must be used for. Every
  entry in either must name the registry page it came from; a rule with no
  citable source stays out. Keep them apart — a requirement reported as an
  eligibility rule claims a gate on the buyer that is not there.
* `tld-categories.json` — what a TLD is *for*. Listed only where the name plainly
  says so, brand TLDs excluded wholesale, a TLD may sit in several subjects.

Prefer `--dry-run` while working on a harvester, and say in the PR description
when a data file was regenerated and by which script.

## Documentation

`docs/` is canonical — it ships inside every archive and ds.aminul.dev generates
its pages from it, so `site/src/content/docs/` and `site/src/content/pages/` are
build output and are never edited by hand.

* New page: create `docs/<slug>.md` with a `# Title` heading, then add the slug
  to `SIDEBAR` in `site/scripts/build-docs.mjs` — that array is the sidebar and
  its order.
* Link between pages with plain relative links (`prices.md`,
  `choosing-the-source.md#your-own-prices`); the site rewrites them to routes.
  Link to a repo file with `../` (`../whois.json`); those become GitHub links.
* **A new or changed flag belongs in three places**: `--help` in `src/main.rs`,
  the matching page under `docs/`, and `ds.1`.
* `site/src/data/root-zone.json` is gitignored (~700 KB, serial moves most days)
  and fetched by the Pages workflow on each deploy; locally an earlier build's
  copy is reused for a day, and `node site/scripts/build-root-zone.mjs --force`
  refetches. `rdap-dns.json` beside it *is* committed so a build works when IANA
  is unreachable.

## Commits

Subjects are a sentence describing the outcome, in the imperative, with no
`type:` prefix — e.g. "Punycode Unicode TLDs when normalizing, so .বাংলা
resolves". Explain the reasoning in the body, wrapped at ~72 columns. One topic
per pull request. Note anything you could not verify — a registry you could not
reach, a platform you could not test on — rather than leaving a reviewer to find
it. For a bug fix, include the reproducing command and what it printed before
and after.

## Changelog

`CHANGELOG.md` is kept up to date as changes land, not reconstructed at release
time from `git log`. A pull request that changes what somebody using `ds` sees
adds a line under `## [Unreleased]`, in the same voice as the commit subjects —
the outcome, not the files. Grouped **Added** / **Changed** / **Fixed**, with
the PR number where there is one.

Nothing user-visible, nothing to write: a harvester tweak, a test, a refactor
that leaves every answer the same, or a data refresh that only moves rows all
stay out of it.

When a release is cut, `## [Unreleased]` becomes `## [<version>] — <date>`, a
fresh empty `## [Unreleased]` goes above it, and the compare links at the foot
of the file gain a row. `scripts/release.mjs` does not do this for you yet.

## Releases

**Never tag by hand — run `scripts/release.mjs`.** The tag does not set the
version: archive names come from the tag, but the binary's `--version` comes
from `Cargo.toml`, so tagging without bumping first ships `ds-v0.1.8-*.tar.gz`
around a binary that reports the previous version. That has already shipped
twice — `v0.1.6` and `v0.1.7` both point at commits whose `Cargo.toml` still
said `0.1.5`.

```sh
node scripts/release.mjs patch --dry-run   # print the plan, change nothing
node scripts/release.mjs patch             # prepare: edit, verify, commit, tag
node scripts/release.mjs 0.2.0 --push      # ...and push main and the tag
```

It bumps `Cargo.toml`, refreshes the `ds` entry in `Cargo.lock` (via `cargo
metadata`, which rewrites the lock without compiling), rewrites the `.TH` line
in `ds.1`, runs the whole offline check list, then commits `Release <version>`
and tags `v<version>`. It refuses to start on a non-`main` branch, on a dirty
tree, on a tag that already exists locally or on origin, or on a version that is
not ahead of the current one — and if any check fails, nothing is committed.

`--push` is opt-in and never implied, because pushing the tag publishes a public
release across ten targets. Without it the script stops at a local commit and
tag and prints both the push command and the undo. `--skip-checks` skips the
test run; `--allow-branch` permits a release from somewhere other than `main`.

Two things stay manual, and the script says so on the way out:

* `Formula/ds.rb` in the [aminulbd/homebrew-tap](https://github.com/aminulbd/homebrew-tap)
  repo needs the new URLs and the SHA-256 of archives that do not exist until
  the workflow has built them. The formula is not mirrored in this repo — see
  `packaging/homebrew/README.md`.
* `site/src/data/release.json` is generated from the GitHub API by
  `site/scripts/build-release.mjs` — never hand-edit it.

The release job hard-fails on a tag/version mismatch, so a hand-tagged release
now stops before publishing rather than warning into a green run.
`workflow_dispatch` runs the same build matrix without publishing, which is the
way to check a build before tagging.
