# Changelog

Every released version of `ds`, newest first, in the same terms the commits use:
what changed for somebody using the tool, not which files moved. Entries are
written when the change lands, and `## [Unreleased]` becomes the next version's
heading when `scripts/release.mjs` cuts it.

The layout follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions are `MAJOR.MINOR.PATCH`, and while `ds` is below `1.0.0` a minor bump
is where a feature lands and a patch is everything else.

## [Unreleased]

Nothing yet.

## [0.1.9] — 2026-09-04

### Added

- A Docker image, `ghcr.io/aminulbd/ds`, built for amd64 and arm64 and
  published on every tag. It is Alpine around a static binary, about 22 MB, and
  its default command is the HTTP API — see
  [Install](docs/install.md#docker).
- `eligibility.json` now records what a registry asks of the *name*, not only
  who may buy it: an HTTPS-only zone, or a rule about what the name must be used
  for. Kept apart from the eligibility rules, since a requirement reported as an
  eligibility rule would claim a gate on the buyer that is not there (#35).

### Fixed

- The site is deployed after a release rather than six minutes before it, so its
  download links no longer point at the previous version for the length of a
  build.
- `scripts/release.mjs` pushes the release commit to `main` whatever the local
  branch is called, which is what `--allow-branch` always meant to do.

## [0.1.8] — 2026-08-26

### Added

- `scripts/release.mjs`, which bumps `Cargo.toml`, `Cargo.lock` and `ds.1`,
  runs the whole offline check list and only then commits and tags — so the tag
  can no longer outrun the version the binary reports. The release workflow
  hard-fails on a mismatch rather than warning into a green run.
- A page naming every TLD each "who can register" rule reaches.
- `CLAUDE.md`, so Claude Code starts from the same rules a contributor does.

### Changed

- The Homebrew formula is no longer vendored here; the
  [aminulbd/homebrew-tap](https://github.com/aminulbd/homebrew-tap) repo owns it.

## [0.1.7] — 2026-08-26

### Added

- The HTTP API ships in every build, behind `--serve`, instead of needing a
  feature flag at compile time. `--no-default-features` still builds the CLI
  alone.
- The root zone is read at build time and published on the site.
- The README is broken into per-topic pages under `docs/`, which ship inside
  every archive, with a `CONTRIBUTING.md` beside them.

### Changed

- The TLD table on the site is paged, and its header stays put while you scroll.

> The archives published for this tag hold a binary that reports `0.1.5`: the
> tag was cut before `Cargo.toml` was bumped. `scripts/release.mjs` in 0.1.8
> exists so this cannot happen again.

## [0.1.6] — 2026-08-25

### Added

- The A–Z of TLDs: what each one is, what it is for and who runs it, with a page
  for every TLD (#30, #31).
- Prices for 870 TLDs from three registrars rather than one, including the `.bd`
  family priced from the registry with its taka converted (#26).
- `--where` says where a TLD can actually be registered, and the site shows
  where it can be bought and who may buy it (#28).
- An optional HTTP API.
- `whois.json` is harvested from the IANA root database, with each row traceable
  to its source.

### Fixed

- Unicode TLDs are punycoded when normalized, so `.বাংলা` resolves (#29).
- A brand or reserved TLD is reported `PRIVATE` rather than `AVAILABLE`: "no
  such domain" in a zone nobody may register in is not an offer.
- The installer creates the directories it needs and says when they are not on
  your `PATH`.

> As with 0.1.7, the binary in these archives reports `0.1.5`.

## [0.1.5] — 2026-08-18

### Added

- `--json` saves the results to a file when given one (#7).
- `-v` prints the version, as `-V` already did.

### Changed

- Homebrew is offered first in the install instructions, and follows the tap
  rename to `aminulbd/tap`.

## [0.1.4] — 2026-08-18

### Added

- Results show what each TLD costs.
- The Windows installer can install for one user or for everyone.

### Fixed

- Slashes are kept out of the artifact names, so a branch build no longer names
  archives after a directory that does not exist.

## [0.1.3] — 2026-08-17

### Fixed

- DNS falls back to TCP when UDP gets no answer.
- DNS is resolved in process, so lookups work on Termux and Android.
- ANSI colours are enabled on the Windows console.

## [0.1.2] — 2026-08-15

### Added

- A custom WHOIS server table can be loaded from `whois.json`.

### Changed

- Releasing is idempotent, and a re-pushed tag supersedes the run already going
  for it rather than racing it.

## [0.1.1] — 2026-08-15

### Added

- A custom RDAP server list can be loaded from `rdap.json`, in the bootstrap
  format IANA publishes and no other.
- `.deb`, `.rpm`, `.dmg` and `.msi` installers are built on release.

## [0.1.0] — 2026-08-15

The first release, as `ds`.

- Checks whether domains are registered over RDAP, falling back to WHOIS on port
  43, and reports a lookup it cannot answer as unknown rather than guessing.
- `--tld`, `--cctld`, `--tld-len` and `--level` choose what to check;
  `--source` pins lookups to one data source; `--where` says where an available
  domain can be registered.
- A man page, an MIT licence, and binaries published whenever a `v*` tag is
  pushed.

[Unreleased]: https://github.com/AminulBD/ds/compare/v0.1.9...HEAD
[0.1.9]: https://github.com/AminulBD/ds/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/AminulBD/ds/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/AminulBD/ds/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/AminulBD/ds/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/AminulBD/ds/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/AminulBD/ds/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/AminulBD/ds/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/AminulBD/ds/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/AminulBD/ds/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/AminulBD/ds/releases/tag/v0.1.0
