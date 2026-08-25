#!/usr/bin/env node
//
// Builds tld-facts.json — the A–Z list of every delegated TLD, what kind it is, who
// runs it, and the categories that follow from those facts.
//
//   node scripts/harvest-tld-facts.mjs            # rebuild tld-facts.json
//   node scripts/harvest-tld-facts.mjs --dry-run  # fetch, report, write nothing
//
// Refs #27.
//
// WHERE THIS COMES FROM
//
// An A–Z of TLDs with their type and sponsoring organisation is not any one
// site's data — it is the root zone, and IANA publishes it. Three requests,
// all public domain, all machine-readable, none behind a challenge:
//
//   https://data.iana.org/TLD/tlds-alpha-by-domain.txt
//       The authoritative set. Carries a serial in its header comment, which
//       is recorded so a reader can tell which root zone this file describes.
//
//   https://www.iana.org/domains/root/db
//       The Root Zone Database. One page, one row per TLD: the punycode label
//       in the link, the unicode one in the text, the `type` (generic,
//       country-code, sponsored, generic-restricted, infrastructure, test),
//       and the sponsoring organisation. `robots.txt` is `Disallow:` — empty,
//       meaning everything is allowed — and there is no challenge in front of
//       it. The same page IANA points the public at.
//
//   https://www.icann.org/resources/registries/gtlds/v2/gtlds.json
//       Already used by scripts/build-private-tlds.mjs. Adds, for gTLDs only,
//       the registry operator under contract, the delegation date, whether the
//       contract has been terminated, and the Specification 13 flag that marks
//       a brand TLD.
//
// CATEGORIES, AND WHOSE THEY ARE
//
// Every category in the output is *derived from those three sources* and says
// which fact produced it — `country-code` because IANA types it so, `brand`
// because ICANN recorded Specification 13, `new-gtld` because ICANN delegated
// it after the 2013 round opened. Nothing here is a topical taxonomy of the
// "Food & Drink" sort. That kind of grouping is editorial judgement, it is the
// work of whoever made it, and it is not derivable from the root zone — so it
// is not invented here and not copied from anyone. If ds ever wants one, it
// should be hand-maintained in the repo like eligibility.json, with its own
// reasoning attached.

import { writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..');

const UA = 'ds-tld-facts/1.0 (+https://github.com/AminulBD/ds; TLD facts table for the ds CLI)';
const ROOT_ZONE = 'https://data.iana.org/TLD/tlds-alpha-by-domain.txt';
const ROOT_DB = 'https://www.iana.org/domains/root/db';
const ICANN = 'https://www.icann.org/resources/registries/gtlds/v2/gtlds.json';

// The first new-gTLD delegations of the 2013 round. Anything delegated on or
// after this is a new gTLD; .com and the other seven originals are not.
const NEW_GTLD_ERA = '2013-10-23';

const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i === -1 ? fallback : args[i + 1];
};
const opts = {
  out: resolve(repo, flag('out', 'tld-facts.json')),
  dryRun: args.includes('--dry-run'),
};

if (args.includes('--help')) {
  console.log('usage: node scripts/harvest-tld-facts.mjs [--out tld-facts.json] [--dry-run]');
  process.exit(0);
}

async function get(url, { as = 'text' } = {}) {
  const res = await fetch(url, {
    headers: { 'user-agent': UA, accept: as === 'json' ? 'application/json' : 'text/html,text/plain,*/*' },
    signal: AbortSignal.timeout(60_000),
  });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return as === 'json' ? res.json() : res.text();
}

/** Strip tags and decode the handful of entities IANA's table actually uses. */
const text = (html) =>
  String(html)
    .replace(/<[^>]*>/g, '')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#0?39;|&apos;/g, "'")
    .replace(/&nbsp;/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();

/** The authoritative set, plus the serial IANA stamps into the header. */
async function rootZone() {
  const body = await get(ROOT_ZONE);
  const lines = body.split('\n');
  const version = lines.find((l) => l.startsWith('#'))?.replace(/^#\s*/, '').trim() ?? null;
  const tlds = new Set(lines.map((l) => l.trim().toLowerCase()).filter((l) => l && !l.startsWith('#')));
  console.log(`  root     ${tlds.size} TLDs — ${version ?? 'no version line'}`);
  return { tlds, version };
}

/**
 * The Root Zone Database table. One row per TLD:
 *
 *   <td><span class="domain tld"><a href="/domains/root/db/xn--11b4c3d.html">.कॉम</a></span></td>
 *   <td>generic</td>
 *   <td>VeriSign Sarl</td>
 *
 * The href is what carries the punycode, so an IDN is keyed the way the rest
 * of this repo keys it and the unicode label rides along beside it. Rows
 * without that span are the page's own navigation tables, not TLDs.
 */
export function parseRootDb(html) {
  const out = new Map();
  for (const row of html.match(/<tr>[\s\S]*?<\/tr>/g) ?? []) {
    const link = row.match(/class="domain tld"[^>]*>\s*<a\s+href="[^"]*\/db\/([a-z0-9-]+)\.html"[^>]*>([^<]*)<\/a>/i);
    if (!link) continue;
    const tld = link[1].toLowerCase();
    const label = link[2].trim().replace(/^\.+/, '');
    const cells = (row.match(/<td[^>]*>[\s\S]*?<\/td>/g) ?? []).map(text);
    const type = (cells[1] ?? '').toLowerCase();
    const sponsor = cells[2] ?? '';
    out.set(tld, {
      type: type || null,
      // "Not assigned" is IANA saying the TLD is delegated but has no operator.
      // That is a fact worth keeping, not a name to record as if it were one.
      sponsor: sponsor && sponsor !== 'Not assigned' ? sponsor : null,
      assigned: sponsor !== 'Not assigned',
      ...(label && label.toLowerCase() !== tld ? { unicode: label } : {}),
    });
  }
  return out;
}

/** What parseRootDb found, for the run log. Kept out of the parser so a test can call it quietly. */
const describeRootDb = (db) => {
  const types = {};
  for (const r of db.values()) types[r.type] = (types[r.type] ?? 0) + 1;
  return `${db.size} TLDs — ${Object.entries(types).map(([t, n]) => `${n} ${t}`).join(', ')}`;
};

/** gTLDs under ICANN contract: who runs them, when they were delegated, and whether it is a brand. */
async function icannGtlds() {
  const json = await get(ICANN, { as: 'json' });
  const out = new Map();
  for (const g of json.gTLDs ?? []) {
    const tld = String(g.gTLD ?? '').trim().toLowerCase();
    if (!tld) continue;
    out.set(tld, g);
  }
  console.log(`  icann    ${out.size} gTLDs under contract, updated ${String(json.updatedOn ?? '').slice(0, 10)}`);
  return { gtlds: out, updatedOn: json.updatedOn ?? null };
}

/**
 * Why each category exists, stated once. Every rule names the source field it
 * reads, and every field it names is on the record beside the category — so a
 * reader who disagrees with a classification can check it against the same
 * fact the script used, without the reasoning being repeated 1,595 times.
 */
export const CATEGORY_RULES = {
  generic: 'IANA Root Zone Database types it "generic"',
  'country-code': 'IANA Root Zone Database types it "country-code"',
  sponsored: 'IANA Root Zone Database types it "sponsored"',
  'generic-restricted': 'IANA Root Zone Database types it "generic-restricted"',
  infrastructure: 'IANA Root Zone Database types it "infrastructure"',
  test: 'IANA Root Zone Database types it "test" — an IDN evaluation TLD, not registrable',
  idn: 'the label is punycode — an internationalised TLD; `unicode` is how it reads',
  unassigned: 'IANA lists no sponsoring organisation for it',
  'removed-from-root': 'IANA still records it but it is not in the current root zone — it used to resolve and no longer does',
  brand: 'ICANN records Specification 13 — a brand TLD, closed to the public',
  'new-gtld': `ICANN delegated it on or after ${NEW_GTLD_ERA}, when the 2013 round opened — see \`delegated\``,
  'legacy-gtld': `ICANN delegated it before ${NEW_GTLD_ERA} — see \`delegated\``,
  'contract-terminated': 'ICANN records the registry contract as terminated',
  removed: 'ICANN records a removal date — see `removed`',
  'third-level': 'ICANN records registrations at the third level or lower',
};

/**
 * The categories a TLD belongs to. Each one is derived from a source fact, and
 * CATEGORY_RULES says which; nothing here is an opinion about what a TLD is
 * *for*.
 */
export function categorize(tld, { rootDb, icann, inRootZone }) {
  const cats = [];

  if (rootDb?.type) cats.push(rootDb.type);
  if (tld.startsWith('xn--')) cats.push('idn');
  if (rootDb && !rootDb.assigned) cats.push('unassigned');
  if (!inRootZone) cats.push('removed-from-root');

  if (icann) {
    if (icann.specification13 === true) cats.push('brand');
    if (icann.delegationDate) cats.push(icann.delegationDate >= NEW_GTLD_ERA ? 'new-gtld' : 'legacy-gtld');
    if (icann.contractTerminated === true) cats.push('contract-terminated');
    if (icann.removalDate) cats.push('removed');
    if (icann.thirdOrLowerLevelRegistration === true) cats.push('third-level');
  }

  // Only categories this file explains. An unexplained one would be a label
  // with nothing behind it, which is the thing this table is trying not to be.
  return cats.filter((c) => c in CATEGORY_RULES);
}

// --- run -------------------------------------------------------------------

async function main() {
  const [{ tlds: rootTlds, version }, rootDbHtml, { gtlds, updatedOn }] = await Promise.all([
    rootZone(),
    get(ROOT_DB),
    icannGtlds(),
  ]);
  const rootDb = parseRootDb(rootDbHtml);
  console.log(`  rootdb   ${describeRootDb(rootDb)}`);

  // A reshaped page would otherwise write a near-empty table and look fine.
  if (rootDb.size < rootTlds.size / 2) {
    console.error(
      `root database parsed ${rootDb.size} TLDs against ${rootTlds.size} in the root zone — refusing to write. ` +
        'The page has most likely been reshaped.',
    );
    process.exit(1);
  }

  // The A-Z is the union: everything delegated today, plus everything IANA still
  // records that no longer is. `in_root_zone` is what separates the two, so a
  // caller can take the live set without having to know the history.
  const every = [...new Set([...rootTlds, ...rootDb.keys()])].sort();
  const table = {};
  const onlyInDb = [];
  for (const tld of every) {
    const db = rootDb.get(tld) ?? null;
    const g = gtlds.get(tld) ?? null;
    const inRootZone = rootTlds.has(tld);
    if (!inRootZone) onlyInDb.push(tld);
    table[tld] = {
      ...(db?.unicode ? { unicode: db.unicode } : {}),
      ...(db?.type ? { type: db.type } : {}),
      ...(db?.sponsor ? { sponsor: db.sponsor } : {}),
      ...(g?.registryOperator ? { registry: g.registryOperator } : {}),
      ...(g?.delegationDate ? { delegated: g.delegationDate } : {}),
      ...(g?.removalDate ? { removed: g.removalDate } : {}),
      in_root_zone: inRootZone,
      categories: categorize(tld, { rootDb: db, icann: g, inRootZone }),
    };
  }

  // --- report ----------------------------------------------------------------

  const counts = {};
  for (const record of Object.values(table)) for (const c of record.categories) counts[c] = (counts[c] ?? 0) + 1;

  const missing = [...rootTlds].filter((t) => !rootDb.has(t));
  console.log('');
  console.log(`  built    ${Object.keys(table).length} TLDs — ${rootTlds.size} in the current root zone`);
  for (const [id, n] of Object.entries(counts).sort((a, b) => b[1] - a[1])) {
    console.log(`           ${String(n).padStart(5)}  ${id}`);
  }
  if (missing.length) {
    console.log(`\n  NOTE     ${missing.length} in the root zone but not in the Root Zone Database: ${missing.slice(0, 10).join(', ')}`);
  }
  if (onlyInDb.length) {
    console.log(`  listed   ${onlyInDb.length} kept as removed-from-root: ${onlyInDb.slice(0, 8).join(', ')}, ...`);
  }

  const out = {
    generated: new Date().toISOString().slice(0, 10),
    sources: {
      root_zone: `${ROOT_ZONE} (${version ?? 'no version line'})`,
      root_database: `${ROOT_DB} — type and sponsoring organisation`,
      icann: `${ICANN} — registry operator, delegation date, Specification 13; updated ${String(updatedOn ?? '').slice(0, 10)}`,
    },
    _about: [
      'The A-Z of every TLD IANA records: what kind it is, who runs it, and',
      'whether it is still in the root zone. Every category is derived from the',
      'sources above; `categories` below says which fact each one rests on, and',
      'that fact is on the record beside it.',
      '',
      'There is no topical taxonomy here — no "Food & Drink", no "Tech". That',
      'kind of grouping is editorial judgement, it is not derivable from the',
      'root zone, and it is not copied from anyone who has made one. If ds ever',
      'wants one it should be hand-maintained here like eligibility.json.',
      '',
      'Generated by scripts/harvest-tld-facts.mjs. Do not edit by hand.',
    ],
    categories: CATEGORY_RULES,
    tlds: table,
  };

  if (opts.dryRun) {
    console.log('\n  --dry-run: nothing written');
  } else {
    await writeFile(opts.out, JSON.stringify(out, null, 2) + '\n');
    console.log(`\n  wrote    ${opts.out}`);
  }
}

// Importing this file for its parser must not fire a network request, which is
// what lets scripts/test-tld-facts-parse.mjs run offline.
if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  main().catch((err) => {
    console.error(`\n${err.message}`);
    process.exit(1);
  });
}
