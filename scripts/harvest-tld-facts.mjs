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
// REGISTRY FACTS (--deep)
//
// Those three requests say what a TLD *is*. Who actually runs it, and how you
// reach them, lives one page deeper: IANA publishes a delegation record per
// TLD at /domains/root/db/<tld>.html carrying the manager, the registry's own
// URL, its WHOIS and RDAP servers, the authoritative name servers, and the
// dates the delegation was registered and last updated.
//
// `--deep` fetches those, and it is off by default because it is ~1,600 pages
// rather than three. It is deliberately **sequential** — one request at a
// time, `--gap` seconds apart, never concurrent — and every page is cached
// under scripts/.iana-cache/ so a re-run costs nothing. IANA's robots.txt is
// `Disallow:` with an empty value, meaning everything is permitted, and there
// is no challenge in front of any of it; the pacing is courtesy, not evasion.
//
// WHAT IS DELIBERATELY NOT TAKEN
//
// Each delegation record also carries an administrative and a technical
// contact: a person's name, email address and telephone number. For most gTLDs
// that is a role account, but for plenty of ccTLDs it is a named individual.
// None of it is collected. It would be personal data compiled out of 1,600
// pages into a file committed to a public repository, and a domain-search tool
// has no use for it. The organisation and its country are kept; the people are
// not.
//
// TWO KINDS OF CATEGORY, KEPT APART
//
// `categories` on each record is *derived from the three sources above* and
// says which fact produced it — `country-code` because IANA types it so,
// `brand` because ICANN recorded Specification 13, `new-gtld` because ICANN
// delegated it after the 2013 round opened. Nothing in it is a judgement.
//
// `topics` is the other kind: what a TLD is *for*. No source says .pizza is
// about food — the root zone records who runs a TLD, not what it means — so
// that grouping cannot be derived and is not scraped from anyone who has made
// one. It comes from tld-categories.json, which is hand-maintained in this
// repo like eligibility.json, and it is our reading.
//
// The two never merge. A reader can always tell a fact from an opinion by
// which field it came out of.

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..');

const UA = 'ds-tld-facts/1.0 (+https://github.com/AminulBD/ds; TLD facts table for the ds CLI)';
const ROOT_ZONE = 'https://data.iana.org/TLD/tlds-alpha-by-domain.txt';
const ROOT_DB = 'https://www.iana.org/domains/root/db';
const ICANN = 'https://www.icann.org/resources/registries/gtlds/v2/gtlds.json';
const DELEGATION = (tld) => `https://www.iana.org/domains/root/db/${tld}.html`;
const TOPICS = resolve(repo, 'tld-categories.json');

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
  deep: args.includes('--deep'),
  // Seconds between delegation-record requests. One at a time, so this is the
  // whole of the load: ~1,600 pages at a second each is under half an hour.
  gap: Number(flag('gap', 1)),
  cache: resolve(here, '.iana-cache'),
  refresh: args.includes('--refresh'),
  dryRun: args.includes('--dry-run'),
};

if (args.includes('--help')) {
  console.log(`usage: node scripts/harvest-tld-facts.mjs [--out tld-facts.json] [--dry-run]
       [--deep] [--gap seconds] [--refresh]

  --deep     also fetch each TLD's IANA delegation record: manager, country,
             registry URL, WHOIS and RDAP servers, name servers, dates.
             ~1,600 pages, one at a time, cached under scripts/.iana-cache/.
  --gap      seconds between those requests (default 1)
  --refresh  ignore the cache and refetch`);
  process.exit(0);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function get(url, { as = 'text' } = {}) {
  const res = await fetch(url, {
    headers: { 'user-agent': UA, accept: as === 'json' ? 'application/json' : 'text/html,text/plain,*/*' },
    signal: AbortSignal.timeout(60_000),
  });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return as === 'json' ? res.json() : res.text();
}

/** Decode the entities IANA's pages actually use, numeric ones included. */
const entities = (s) =>
  String(s)
    .replace(/&#x([0-9a-f]+);/gi, (_, h) => String.fromCodePoint(parseInt(h, 16)))
    .replace(/&#(\d+);/g, (_, d) => String.fromCodePoint(Number(d)))
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&nbsp;/g, ' ')
    // Last, so a doubly-encoded "&amp;#x27;" does not become an apostrophe.
    .replace(/&amp;/g, '&');

/** Strip tags and decode entities. */
const text = (html) =>
  entities(String(html)
    .replace(/<[^>]*>/g, ''))
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
 * One TLD's IANA delegation record — the page behind each row of the Root Zone
 * Database.
 *
 * Headings differ by kind: a gTLD's operator sits under "Sponsoring
 * Organisation", a ccTLD's under "ccTLD Manager". Both are followed by the
 * organisation's name, its postal address, and its country on the last line.
 * Only the name and the country are kept — a street address is of no use to a
 * domain search and the file is committed.
 *
 * "Administrative Contact" and "Technical Contact" are skipped entirely: they
 * carry a person's name, email and telephone number, and compiling those out
 * of 1,600 pages into a public repository is not something this script does.
 */
export function parseDelegation(html) {
  const flat = entities(String(html).replace(/<[^>]*>/g, '\n'));
  const lines = flat.split('\n').map((l) => l.trim());

  // Every heading, so a section can be read up to whichever comes next.
  const HEADINGS = [
    'Sponsoring Organisation', 'ccTLD Manager', 'Administrative Contact',
    'Technical Contact', 'Name Servers', 'Registry Information',
  ];
  const section = (heading) => {
    const i = lines.indexOf(heading);
    if (i === -1) return [];
    let end = lines.length;
    for (const h of HEADINGS) {
      const j = lines.indexOf(h, i + 1);
      if (j !== -1 && j < end) end = j;
    }
    return lines.slice(i + 1, end).filter(Boolean);
  };

  const org = section('Sponsoring Organisation').length
    ? section('Sponsoring Organisation')
    : section('ccTLD Manager');

  // The name-server table interleaves host names with IPv4 and IPv6 addresses;
  // a host name is the only cell that looks like a domain.
  const nameservers = [
    ...new Set(
      section('Name Servers')
        .filter((l) => /^[a-z0-9][a-z0-9.-]*\.[a-z]{2,}$/i.test(l))
        .map((l) => l.toLowerCase()),
    ),
  ].sort();

  // A label and its value are separate text nodes, so the value is whatever
  // non-empty line follows — unless that turns out to be the next heading,
  // which means the label had no value at all.
  const after = (label) => {
    const i = lines.indexOf(label);
    if (i === -1) return null;
    const value = lines.slice(i + 1).find(Boolean) ?? null;
    return value && !HEADINGS.includes(value) ? value : null;
  };

  const url = after('URL for registration services:');
  const whois = after('WHOIS Server:');
  const rdap = after('RDAP Server:');
  const registered = flat.match(/Registration date (\d{4}-\d{2}-\d{2})/)?.[1] ?? null;
  const updated = flat.match(/Record last updated (\d{4}-\d{2}-\d{2})/)?.[1] ?? null;

  const out = {
    ...(org.length ? { manager: org[0] } : {}),
    // The country is the last address line. A one-line block is a name with no
    // address, so there is no country to take from it.
    ...(org.length > 1 ? { country: org.at(-1) } : {}),
    ...(url ? { url } : {}),
    ...(whois ? { whois: whois.toLowerCase() } : {}),
    ...(rdap ? { rdap } : {}),
    ...(nameservers.length ? { nameservers } : {}),
    ...(registered ? { registered } : {}),
    ...(updated ? { updated } : {}),
  };
  return Object.keys(out).length ? out : null;
}

/**
 * Fetch every delegation record, one at a time.
 *
 * Sequential on purpose — no concurrency at any setting — with `--gap` seconds
 * between requests and every page cached on disk, so a second run over 1,600
 * TLDs makes no requests at all.
 */
async function delegations(tlds) {
  await mkdir(opts.cache, { recursive: true });
  const out = new Map();
  let fetched = 0;
  let cached = 0;
  let failed = 0;

  for (const [n, tld] of tlds.entries()) {
    const file = resolve(opts.cache, `${tld}.html`);
    let html = null;

    if (!opts.refresh) {
      html = await readFile(file, 'utf8').catch(() => null);
      if (html !== null) cached++;
    }
    if (html === null) {
      if (fetched) await sleep(opts.gap * 1000);
      try {
        html = await get(DELEGATION(tld));
        fetched++;
        await writeFile(file, html);
      } catch (err) {
        // A TLD whose record will not load is one TLD without these fields,
        // not a reason to lose the other 1,594.
        failed++;
        console.warn(`           WARNING: ${tld}: ${err.message}`);
        continue;
      }
    }

    const record = parseDelegation(html);
    if (record) out.set(tld, record);
    if ((n + 1) % 100 === 0 || n + 1 === tlds.length) {
      console.log(`           ${n + 1}/${tlds.length} — ${fetched} fetched, ${cached} cached, ${out.size} parsed`);
    }
  }

  console.log(`  deep     ${out.size} delegation records${failed ? ` (${failed} failed)` : ''}`);
  return out;
}

/**
 * tld-categories.json, inverted into `tld -> [topic, ...]`.
 *
 * The file is grouped by topic because that is how a person maintains it; the
 * table wants it the other way round. Topics stay in the file's own order, so
 * a record reads the way the taxonomy is written rather than alphabetically.
 */
export function topicIndex(taxonomy) {
  const index = new Map();
  for (const [id, category] of Object.entries(taxonomy?.categories ?? {})) {
    for (const tld of category.tlds ?? []) {
      if (!index.has(tld)) index.set(tld, []);
      if (!index.get(tld).includes(id)) index.get(tld).push(id);
    }
  }
  return index;
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

  const taxonomy = JSON.parse(await readFile(TOPICS, 'utf8'));
  const topics = topicIndex(taxonomy);
  console.log(`  topics   ${topics.size} TLDs across ${Object.keys(taxonomy.categories).length} subjects (${TOPICS.split('/').pop()})`);

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

  // The delegation records, if asked for. Done after the cheap sources so a
  // reshaped root database fails the run before 1,600 requests are made.
  const deep = opts.deep ? await delegations(every) : new Map();

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
      // From the TLD's own IANA delegation record, under --deep. Absent
      // otherwise, so a shallow run does not read as a registry with nothing
      // published about it.
      ...(deep.has(tld) ? { delegation: deep.get(tld) } : {}),
      // What the TLD is for, from the hand-maintained taxonomy. Deliberately a
      // separate field from `categories`: one is a fact, the other a reading.
      ...(topics.has(tld) ? { topics: topics.get(tld) } : {}),
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
  if (opts.deep) {
    const have = (field) => Object.values(table).filter((r) => r.delegation?.[field]).length;
    console.log('');
    console.log(`  registry ${Object.values(table).filter((r) => r.delegation).length} TLDs with a delegation record`);
    console.log(`           ${have('manager')} name a manager, ${have('country')} a country`);
    console.log(`           ${have('url')} publish a registration URL`);
    console.log(`           ${have('whois')} a WHOIS server, ${have('rdap')} an RDAP service`);
    console.log(`           ${have('nameservers')} list name servers`);
  }

  // A topic naming a TLD that is not in the root database is a typo, and it
  // would sit in the file forever looking like data. Say so.
  const phantom = [...topics.keys()].filter((t) => !(t in table));
  if (phantom.length) {
    console.log(`\n  WARNING  ${TOPICS.split('/').pop()} names ${phantom.length} TLD(s) IANA does not list: ${phantom.join(', ')}`);
  }

  const classifiable = Object.entries(table).filter(
    ([, r]) => r.in_root_zone && !r.categories.includes('brand') &&
      ['generic', 'sponsored', 'generic-restricted'].includes(r.type),
  );
  const classified = classifiable.filter(([t]) => topics.has(t)).length;
  console.log('');
  console.log(`  topics   ${classified}/${classifiable.length} classifiable TLDs have a subject`);
  const perTopic = new Map();
  for (const record of Object.values(table)) {
    for (const t of record.topics ?? []) perTopic.set(t, (perTopic.get(t) ?? 0) + 1);
  }
  for (const [id, n] of [...perTopic].sort((a, b) => b[1] - a[1])) {
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
      ...(opts.deep
        ? {
            delegation_records:
              `${DELEGATION('<tld>')} — manager, country, registry URL, WHOIS and RDAP servers, name servers, dates`,
          }
        : {}),
    },
    _about: [
      'The A-Z of every TLD IANA records: what kind it is, who runs it, what it',
      'is for, and whether it is still in the root zone.',
      '',
      '`categories` on a record is derived from the sources above, and the',
      '`categories` block says which fact each one rests on — that fact is on',
      'the record beside it.',
      '',
      '`topics` is not derived. No source says what a TLD is *for*, so that',
      'comes from tld-categories.json, hand-maintained in this repo, and it is',
      'our reading rather than a fact. The two are separate fields precisely so',
      'a reader can tell them apart.',
      '',
      '`delegation`, where present, is from the TLD\'s own IANA delegation',
      'record (harvested with --deep). The administrative and technical',
      'contacts on those pages are deliberately not collected: they are',
      'personal data, and a domain search has no use for them.',
      '',
      'Generated by scripts/harvest-tld-facts.mjs. Do not edit by hand.',
    ],
    categories: CATEGORY_RULES,
    // The taxonomy's own names and descriptions, so `topics` on a record reads
    // without having to open tld-categories.json.
    topics: Object.fromEntries(
      Object.entries(taxonomy.categories).map(([id, c]) => [id, { name: c.name, desc: c.desc }]),
    ),
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
