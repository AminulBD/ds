#!/usr/bin/env node
//
// Prepares a release: bumps the version everywhere it is written by hand,
// verifies the tree, then commits and tags.
//
//   node scripts/release.mjs 0.1.8          # an exact version
//   node scripts/release.mjs patch          # 0.1.7 -> 0.1.8
//   node scripts/release.mjs minor          # 0.1.7 -> 0.2.0
//   node scripts/release.mjs patch --dry-run    # print the plan, change nothing
//   node scripts/release.mjs patch --push       # ...and push main and the tag
//
//   --skip-checks     skip cargo test/clippy/fmt and the two script tests
//   --allow-branch    release from a branch other than main. Needed when main
//                     is checked out in another worktree, since git will not
//                     check it out twice. The commit is still pushed to main.
//
// WHY THIS EXISTS
//
// The tag does not set the version. Archive names are built from the tag, but
// the binary's `--version` comes from Cargo.toml, so tagging without bumping
// first ships ds-v0.1.8-*.tar.gz containing a binary that reports 0.1.7. That
// has already happened twice: v0.1.6 and v0.1.7 both point at commits whose
// Cargo.toml still said 0.1.5. The release workflow compares the two but only
// emits a ::warning::, which nobody reads in a green run.
//
// So the ordering is the whole point: bump, refresh the lock, fix the man
// page, verify, commit, and only then tag the commit that carries the bump.
//
// WHAT IT DOES NOT DO
//
// Pushing a tag publishes a public release to ten platforms, so `--push` is
// opt-in and never implied. Without it the script stops at a local commit and
// tag, and prints the push command for you to run.
//
// The Homebrew formula is not touched: it lives in the aminulbd/homebrew-tap
// repo, not this one, and needs the SHA-256 of archives that do not exist
// until the workflow has built them.
// site/src/data/release.json is not touched either -- it is generated from
// the GitHub API by site/scripts/build-release.mjs.

import { readFile, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const argv = process.argv.slice(2);
const flags = new Set(argv.filter((a) => a.startsWith('--')));
const positional = argv.filter((a) => !a.startsWith('--'));

const dryRun = flags.has('--dry-run');
const push = flags.has('--push');
const skipChecks = flags.has('--skip-checks');
const allowBranch = flags.has('--allow-branch');

for (const f of flags) {
  if (!['--dry-run', '--push', '--skip-checks', '--allow-branch'].includes(f)) {
    die(`unknown flag ${f}`);
  }
}

const MONTHS = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December',
];

function die(message) {
  console.error(`release: ${message}`);
  process.exit(1);
}

function git(...args) {
  return execFileSync('git', args, { cwd: repo, encoding: 'utf8' }).trim();
}

function step(message) {
  console.log(`  ${dryRun ? 'would' : '·'} ${message}`);
}

// --- work out the version -----------------------------------------------

const manifestPath = resolve(repo, 'Cargo.toml');
let manifest = await readFile(manifestPath, 'utf8');

// Only the [package] version, which is the first `version =` in the file.
// Every later one is a dependency's.
const currentMatch = manifest.match(/^version = "(\d+)\.(\d+)\.(\d+)"$/m);
if (!currentMatch) die('no [package] version in Cargo.toml');
const current = currentMatch[1] + '.' + currentMatch[2] + '.' + currentMatch[3];
const [maj, min, pat] = currentMatch.slice(1, 4).map(Number);

const spec = positional[0];
if (!spec) {
  die(`no version given (current is ${current}) -- pass 0.1.8, or patch/minor/major`);
}

const next =
  spec === 'patch' ? `${maj}.${min}.${pat + 1}`
  : spec === 'minor' ? `${maj}.${min + 1}.0`
  : spec === 'major' ? `${maj + 1}.0.0`
  : spec.replace(/^v/, '');

if (!/^\d+\.\d+\.\d+$/.test(next)) die(`"${spec}" is not a version or patch/minor/major`);

const cmp = (a, b) => {
  const x = a.split('.').map(Number);
  const y = b.split('.').map(Number);
  return x[0] - y[0] || x[1] - y[1] || x[2] - y[2];
};
if (cmp(next, current) <= 0) die(`${next} is not ahead of the current ${current}`);

const tag = `v${next}`;
console.log(`\nds ${current} -> ${next}  (tag ${tag})\n`);

// --- refuse to start from a tree that would produce a confusing release --

const branch = git('rev-parse', '--abbrev-ref', 'HEAD');
if (branch !== 'main' && !allowBranch) {
  die(`on branch ${branch}, not main -- pass --allow-branch if that is deliberate`);
}

if (git('status', '--porcelain')) {
  die('the working tree is dirty; commit or set aside your changes first');
}

const tags = git('tag', '--list').split('\n');
if (tags.includes(tag)) die(`${tag} already exists`);

// A tag that is already published cannot be moved, so check the remote too
// rather than only the local tag list.
try {
  if (git('ls-remote', '--tags', 'origin', tag)) {
    die(`${tag} already exists on origin`);
  }
} catch {
  console.log('  (could not reach origin to check for the tag; carrying on)');
}

// --- the edits ------------------------------------------------------------

step(`set version = "${next}" in Cargo.toml`);
manifest = manifest.replace(/^version = "\d+\.\d+\.\d+"$/m, `version = "${next}"`);
if (!dryRun) await writeFile(manifestPath, manifest);

// The man page's .TH line carries the version and the month it was written.
const manPath = resolve(repo, 'ds.1');
const man = await readFile(manPath, 'utf8');
const now = new Date();
const stamp = `${MONTHS[now.getMonth()]} ${now.getFullYear()}`;
const th = `.TH DS 1 "${stamp}" "ds ${next}" "User Commands"`;
if (!/^\.TH DS 1 .*$/m.test(man)) die('no .TH line in ds.1');
step(`set the .TH line in ds.1 to ${stamp}, ds ${next}`);
if (!dryRun) await writeFile(manPath, man.replace(/^\.TH DS 1 .*$/m, th));

// Any cargo command that reads the manifest rewrites the lock; `metadata`
// is the one that does it without compiling anything.
step('refresh the ds entry in Cargo.lock (cargo metadata)');
if (!dryRun) {
  execFileSync('cargo', ['metadata', '--format-version', '1'], {
    cwd: repo,
    stdio: ['ignore', 'ignore', 'inherit'],
  });
  const lock = await readFile(resolve(repo, 'Cargo.lock'), 'utf8');
  if (!lock.includes(`name = "ds"\nversion = "${next}"`)) {
    die('Cargo.lock still holds the old version -- refusing to tag');
  }
}

// --- the offline checks, which CI does not run ---------------------------

if (skipChecks) {
  console.log('\n  skipping cargo test / clippy / fmt (--skip-checks)');
} else if (dryRun) {
  console.log('\n  would run: cargo test, cargo clippy --all-targets, cargo fmt --check,');
  console.log('             cargo build --no-default-features, and the two script tests');
} else {
  const checks = [
    ['cargo', ['fmt', '--check']],
    ['cargo', ['test', '--quiet']],
    ['cargo', ['clippy', '--all-targets', '--quiet']],
    ['cargo', ['build', '--quiet', '--no-default-features']],
    ['python3', ['scripts/test_whois_classify.py']],
    ['node', ['scripts/test-tld-facts-parse.mjs']],
  ];
  for (const [cmd, args] of checks) {
    process.stdout.write(`\n$ ${cmd} ${args.join(' ')}\n`);
    try {
      execFileSync(cmd, args, { cwd: repo, stdio: 'inherit' });
    } catch {
      die(`${cmd} ${args.join(' ')} failed -- nothing has been committed`);
    }
  }
}

// --- commit and tag -------------------------------------------------------

console.log('');
step(`commit "Release ${next}" (Cargo.toml, Cargo.lock, ds.1)`);
step(`tag ${tag} -m "ds ${next}"`);

if (!dryRun) {
  git('add', 'Cargo.toml', 'Cargo.lock', 'ds.1');
  git('commit', '-m', `Release ${next}`);
  git('tag', '-a', tag, '-m', `ds ${next}`);
}

// --- push, only when asked ------------------------------------------------

// A release belongs on main whatever the local branch is called. With
// --allow-branch the commit is sitting somewhere else -- most often because
// main is held by another worktree and cannot be checked out here -- and
// pushing that branch by its own name would publish a stray branch while
// leaving main without the bump. Push the commit *at* main instead.
const refspec = branch === 'main' ? 'main' : 'HEAD:main';

// Two pushes rather than one --follow-tags, so the ordering is visible and
// forced: the commit lands on main first, and only then does the tag go up.
// A tag pushed first would, for as long as the second push took or if it
// failed, point at a commit that is on no branch -- which is how v0.1.7 ended
// up dangling off main's history.
const pushes = [
  ['push', 'origin', refspec],
  ['push', 'origin', tag],
];
const asShell = pushes.map((p) => `    git ${p.join(' ')}`).join('\n');

if (dryRun) {
  console.log('\nnothing was changed (--dry-run).\n');
} else if (push) {
  for (const args of pushes) {
    console.log(`\n$ git ${args.join(' ')}`);
    execFileSync('git', args, { cwd: repo, stdio: 'inherit' });
  }
  console.log(`\npushed. the release workflow is building ${tag}.`);
} else {
  if (refspec !== 'main') {
    console.log(`\nnote: on ${branch}, so the first push puts the commit on main.`);
  }
  console.log(`\nprepared locally. to publish, in this order:\n\n${asShell}\n`);
  console.log(`to undo:\n\n    git tag -d ${tag} && git reset --hard HEAD~1\n`);
}

// After the release exists, the tap's formula needs the new URLs and the
// SHA-256s from the published SHA256SUMS -- see packaging/homebrew/README.md
// for the verify steps.
if (!dryRun) {
  console.log('remember: update Formula/ds.rb in aminulbd/homebrew-tap once the assets are up.\n');
}
