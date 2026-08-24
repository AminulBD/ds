// Emits src/data/release.json — the latest release's real asset list.
//
// Names are NOT constructed from a pattern: cargo-deb and cargo-generate-rpm
// add a package revision (ds_0.1.5-1_amd64.deb) that the archive names do not
// have, so anything guessed from the version alone links to a 404. Ask GitHub
// what actually shipped.

import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
const out = resolve(here, '../src/data/release.json');
const API = 'https://api.github.com/repos/AminulBD/ds/releases/latest';

const version = (await readFile(resolve(repo, 'Cargo.toml'), 'utf8'))
  .match(/^version = "(.+?)"/m)?.[1] ?? '0.0.0';

/** Sort an asset into the row of the download table it belongs in. */
function classify(name) {
  const os =
    /apple-darwin/.test(name) ? 'macOS'
    : /windows/.test(name) ? 'Windows'
    : /\.deb$/.test(name) ? 'Debian, Ubuntu'
    : /\.rpm$/.test(name) ? 'Fedora, RHEL, openSUSE'
    : /linux/.test(name) ? 'Linux'
    : null;
  const arch =
    /aarch64|arm64/.test(name) ? 'arm64'
    : /x86_64|amd64/.test(name) ? 'x86_64'
    : /i686|i386/.test(name) ? 'x86 (32-bit)'
    : null;
  const kind = name.match(/\.(tar\.gz|zip|deb|rpm|dmg|msi)$/)?.[1] ?? null;
  const libc = /-musl/.test(name) ? 'musl' : /-gnu/.test(name) ? 'gnu' : null;
  return { os, arch, kind, libc };
}

async function fetchRelease() {
  const headers = { 'user-agent': 'ds-site-build', accept: 'application/vnd.github+json' };
  if (process.env.GITHUB_TOKEN) headers.authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const res = await fetch(API, { headers, signal: AbortSignal.timeout(15000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

try {
  const rel = await fetchRelease();
  const assets = (rel.assets ?? [])
    .map((a) => ({
      name: a.name,
      url: a.browser_download_url,
      size: a.size,
      ...classify(a.name),
    }))
    .sort((a, b) => a.name.localeCompare(b.name));

  await mkdir(dirname(out), { recursive: true });
  await writeFile(out, JSON.stringify({
    tag: rel.tag_name,
    version: rel.tag_name?.replace(/^v/, '') ?? version,
    published: rel.published_at?.slice(0, 10) ?? null,
    url: rel.html_url,
    checksums: assets.find((a) => a.name === 'SHA256SUMS')?.url ?? null,
    assets: assets.filter((a) => a.kind),
  }, null, 2) + '\n');
  console.log(`  release  ${rel.tag_name} — ${assets.length} assets`);
} catch (err) {
  if (existsSync(out)) {
    console.warn(`  release  WARNING: GitHub fetch failed (${err.message}) — using committed copy`);
  } else {
    // No network and no cache: fall back to the crate version with no asset
    // links at all, so /install still renders and points at the releases page.
    await mkdir(dirname(out), { recursive: true });
    await writeFile(out, JSON.stringify({
      tag: `v${version}`, version, published: null,
      url: 'https://github.com/AminulBD/ds/releases/latest',
      checksums: null, assets: [],
    }, null, 2) + '\n');
    console.warn(`  release  WARNING: GitHub fetch failed (${err.message}) — no asset links`);
  }
}
