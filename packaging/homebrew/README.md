# packaging/homebrew

`ds.rb` is the **source-building** Homebrew formula, kept here for an eventual
[homebrew/core](https://github.com/Homebrew/homebrew-core) submission. It is not
the formula users install today.

Day-to-day installs come from the tap at
[aminulbd/homebrew-ds](https://github.com/aminulbd/homebrew-ds), whose
`Formula/ds.rb` downloads the prebuilt per-platform tarball from the releases
page instead of compiling. homebrew/core does not accept prebuilt formulae —
hence two files.

It lives in this repo rather than in the tap because
`brew test-bot --only-tap-syntax` lints every `.rb` in a tap: a formula outside
`Formula/` is linted as plain Ruby (tripping
`Style/FrozenStringLiteralComment`), and two files declaring
`class Ds < Formula` trip `Lint/DuplicateMethods`.

## Before submitting to homebrew/core

`ds` must first clear the new-formula gates in Homebrew's
`utils/shared_audits.rb`. Self-submissions get a 3x notability multiplier, so an
author-opened PR needs **90 forks, 90 watchers, or 225 stars**, and the repo
must be **30+ days old**. Check the current numbers first:

```sh
gh repo view aminulbd/ds --json stargazerCount,forkCount,watchers,createdAt
```

Then bump `url`/`sha256` to the release being submitted and verify:

```sh
brew install --build-from-source ./ds.rb
brew test ./ds.rb
brew audit --new --strict --online ./ds.rb
```
