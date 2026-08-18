# packaging/homebrew

`ds` is distributed exclusively through the
[aminulbd/homebrew-tap](https://github.com/aminulbd/homebrew-tap) tap. There is
no Homebrew core formula.

The tap's `Formula/ds.rb` installs the matching prebuilt release archive for
macOS (Apple Silicon or Intel) and Linux (ARM or Intel). `brew install --HEAD`
builds the latest `main` branch from source using Rust.

## Install

```sh
brew install aminulbd/tap/ds
```

## Updating the formula

For each release, update the versioned release URLs and SHA-256 checksums in
`Formula/ds.rb` in the tap, then verify:

```sh
brew install --build-from-source aminulbd/tap/ds
brew test aminulbd/tap/ds
brew audit --strict aminulbd/tap/ds
```
