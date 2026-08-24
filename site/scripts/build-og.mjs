// Renders public/og.png — the social preview card. Drawn as SVG and rasterised
// with the sharp that Astro already depends on, so it needs no extra tooling
// and can restate the live TLD counts on every build.

import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const out = resolve(here, '../public/og.png');

const { counts } = JSON.parse(await readFile(resolve(here, '../src/data/tlds.json'), 'utf8'));
const n = (x) => x.toLocaleString('en-US');

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;');
const MONO = 'ui-monospace, SFMono-Regular, Menlo, DejaVu Sans Mono, monospace';
const SANS = 'Helvetica Neue, Helvetica, Arial, DejaVu Sans, sans-serif';

const line = (y, sign, name, status, price, colour) => `
  <text x="92" y="${y}" font-family="${MONO}" font-size="26" fill="${colour}">${sign} ${esc(name)}</text>
  <text x="600" y="${y}" font-family="${MONO}" font-size="26" fill="${colour}">${status}</text>
  <text x="900" y="${y}" font-family="${MONO}" font-size="26" fill="#7d838e" text-anchor="end">${price}</text>`;

const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
  <rect width="1200" height="630" fill="#0f1115"/>
  <rect x="64" y="150" width="1072" height="330" rx="14" fill="#0a0c10" stroke="#262b34"/>
  <text x="64" y="86" font-family="${MONO}" font-size="44" font-weight="700" fill="#e6e9ee">
    <tspan fill="#4ac26b">$</tspan><tspan dx="14">ds</tspan>
  </text>
  <text x="64" y="126" font-family="${SANS}" font-size="25" fill="#9aa1ac">
    Check domain availability across every TLD at once
  </text>
  <text x="92" y="205" font-family="${MONO}" font-size="26" fill="#4ac26b">$ <tspan fill="#d6dae1">ds nimbusforge --tld com,net,io,dev</tspan></text>
  ${line(265, '+', 'nimbusforge.dev', 'AVAILABLE', '$15.98', '#4ac26b')}
  ${line(310, '+', 'nimbusforge.io', 'AVAILABLE', '$65.98', '#4ac26b')}
  ${line(355, '-', 'nimbusforge.net', 'TAKEN', '$14.98', '#f0837a')}
  ${line(400, '-', 'nimbusforge.com', 'TAKEN', '$14.98', '#f0837a')}
  <text x="92" y="450" font-family="${MONO}" font-size="22" fill="#7d838e">
    summary: <tspan fill="#4ac26b">2 available</tspan>  <tspan fill="#f0837a">2 taken</tspan>  0 unknown
  </text>
  <text x="64" y="551" font-family="${SANS}" font-size="23" fill="#e6e9ee">
    ${n(counts.total)} TLDs  ·  ${n(counts.priced)} priced  ·  RDAP first, WHOIS fallback
  </text>
  <text x="64" y="588" font-family="${SANS}" font-size="21" fill="#6e7681">ds.aminul.dev</text>
</svg>`;

try {
  const sharp = (await import('sharp')).default;
  await mkdir(dirname(out), { recursive: true });
  await sharp(Buffer.from(svg)).png({ compressionLevel: 9 }).toFile(out);
  console.log('  og       public/og.png (1200x630)');
} catch (err) {
  if (existsSync(out)) console.warn(`  og       WARNING: render failed (${err.message}) — keeping existing og.png`);
  else console.warn(`  og       WARNING: render failed (${err.message}) — no og.png`);
}
