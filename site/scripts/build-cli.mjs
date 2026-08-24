// Captures `ds --help` into src/data/cli-help.txt so /docs/cli cannot drift
// from the clap definitions in src/main.rs.
//
// Uses whichever binary is already built, else asks cargo to build one. As
// with the IANA bootstrap, a committed copy is the fallback so the site still
// builds on a machine with no Rust toolchain.

import { execFileSync } from 'node:child_process';
import { writeFile, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
const out = resolve(here, '../src/data/cli-help.txt');

const exe = process.platform === 'win32' ? 'ds.exe' : 'ds';
const candidates = [
  resolve(repo, 'target/release', exe),
  resolve(repo, 'target/debug', exe),
];

function capture() {
  for (const bin of candidates) {
    if (existsSync(bin)) return execFileSync(bin, ['--help'], { encoding: 'utf8' });
  }
  // No binary lying around — have cargo make one. NO_COLOR keeps clap's output
  // free of escape codes.
  return execFileSync('cargo', ['run', '--quiet', '--', '--help'], {
    cwd: repo,
    encoding: 'utf8',
    env: { ...process.env, NO_COLOR: '1' },
  });
}

await mkdir(dirname(out), { recursive: true });
try {
  const help = capture().replace(/\r\n/g, '\n').trimEnd();
  if (!help.includes('Usage: ds')) throw new Error('output did not look like ds --help');
  await writeFile(out, help + '\n');
  console.log(`  cli      captured ${help.split('\n').length} lines from ds --help`);
} catch (err) {
  if (!existsSync(out)) throw new Error(`could not run ds --help (${err.message}) and no committed copy at ${out}`);
  console.warn(`  cli      WARNING: could not run ds --help (${err.message}) — using committed copy`);
}
