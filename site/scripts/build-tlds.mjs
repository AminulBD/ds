// Builds src/data/tlds.json: one row per TLD, unioned from the three sources
// `ds` itself consults — the bundled whois.json and pricing.json, plus IANA's
// RDAP bootstrap. Run by `npm run gen` (and so by predev/prebuild).

import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath, domainToUnicode } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
const dataDir = resolve(here, '../src/data');
const BOOTSTRAP_URL = 'https://data.iana.org/rdap/dns.json';
const FALLBACK = resolve(dataDir, 'rdap-dns.json');

/** `.CO.UK` / `co.uk` / `.com` -> `co.uk` / `com`. Mirrors normalize_tld in src/tlds.rs. */
const normalize = (s) => s.trim().replace(/^\.+|\.+$/g, '').toLowerCase();

/**
 * Sort a TLD into one of three mutually exclusive kinds, measured on the last
 * label exactly as the CLI's --tld-len does (the TLD of `co.uk` is `uk`):
 *
 *   cctld  two letters — what ICANN reserves for ISO 3166-1 country codes,
 *          and precisely what `--cctld` (= `--tld-len 2`) selects
 *   idn    an A-label, i.e. punycode: `xn--p1ai` is `.рф`
 *   gtld   everything else
 *
 * Note the IDN bucket also holds a handful of *country* TLDs whose Unicode
 * form is a country code (`.рф`, `.срб`); they are not two ASCII letters, so
 * neither this nor the CLI's --cctld counts them as ccTLDs.
 */
function classify(tld) {
  const apex = tld.split('.').pop();
  if (apex.startsWith('xn--')) return 'idn';
  if ([...apex].length === 2) return 'cctld';
  return 'gtld';
}

const readJson = async (p) => JSON.parse(await readFile(p, 'utf8'));

/**
 * The IANA bootstrap is fetched so the table stays current, but a build must
 * not fail because IANA is unreachable — fall back to the committed copy and
 * refresh it whenever the fetch does succeed.
 */
async function bootstrap() {
  try {
    const res = await fetch(BOOTSTRAP_URL, { signal: AbortSignal.timeout(15000) });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const json = await res.json();
    if (!Array.isArray(json.services)) throw new Error('no services array');
    await writeFile(FALLBACK, JSON.stringify(json, null, 2) + '\n');
    console.log(`  rdap     fetched from IANA (${json.services.length} services)`);
    return json;
  } catch (err) {
    if (!existsSync(FALLBACK)) throw new Error(`IANA fetch failed (${err.message}) and no fallback at ${FALLBACK}`);
    console.warn(`  rdap     WARNING: IANA fetch failed (${err.message}) — using committed fallback`);
    return readJson(FALLBACK);
  }
}

await mkdir(dataDir, { recursive: true });

// --- whois.json: one entry can name many extensions, all sharing a server ---
const whois = new Map();
for (const entry of await readJson(resolve(repo, 'whois.json'))) {
  const uri = entry.uri ?? '';
  const host = uri.startsWith('socket://')
    ? uri.slice('socket://'.length).replace(/\/+$/, '')
    : uri;
  const kind = uri.startsWith('http') ? 'http' : 'socket';
  for (const ext of entry.extensions.split(',')) {
    const tld = normalize(ext);
    if (tld) whois.set(tld, { host, kind });
  }
}

// --- pricing.json: one offer per registrar per TLD; the column is their mean ---
const pricing = new Map();
for (const [key, offers] of Object.entries(await readJson(resolve(repo, 'pricing.json')))) {
  const tld = normalize(key);
  if (!tld) continue;
  const mean = (field) => {
    const xs = offers
      .map((o) => o?.prices?.[field])
      .filter((v) => typeof v === 'number' && Number.isFinite(v));
    return xs.length ? Math.round((xs.reduce((a, b) => a + b, 0) / xs.length) * 100) / 100 : null;
  };
  pricing.set(tld, {
    price: mean('regular'),
    renew: mean('renew'),
    transfer: mean('transfer'),
    registrars: [...new Set(offers.map((o) => o?.register).filter(Boolean))],
  });
}

// --- IANA RDAP bootstrap: [[["com","net"], ["https://..."]], ...] ---
const rdap = new Map();
for (const [tlds, servers] of (await bootstrap()).services) {
  for (const t of tlds) {
    const tld = normalize(t);
    if (tld && servers?.length) rdap.set(tld, servers[0].replace(/\/+$/, ''));
  }
}

const rows = [...new Set([...whois.keys(), ...pricing.keys(), ...rdap.keys()])]
  .sort()
  .map((tld) => {
    const w = whois.get(tld);
    const r = rdap.get(tld);
    const p = pricing.get(tld) ?? {};
    return {
      tld,
      // How many labels the registration sits under: `com` = 2nd level,
      // `co.uk` = 3rd. Matches the --level filter in the CLI.
      level: tld.includes('.') ? 3 : 2,
      kind: classify(tld),
      // The readable form of a punycode TLD: xn--p1ai -> рф. null otherwise.
      unicode: classify(tld) === 'idn' ? domainToUnicode(tld) : null,
      cctld: classify(tld) === 'cctld',
      price: p.price ?? null,
      renew: p.renew ?? null,
      transfer: p.transfer ?? null,
      registrars: p.registrars ?? [],
      // Mirrors the real fall-through: RDAP first, WHOIS second.
      source: r && w ? 'both' : r ? 'rdap' : w ? 'whois' : 'none',
      rdapServer: r ?? null,
      whoisHost: w?.host ?? null,
      whoisKind: w?.kind ?? null,
    };
  });

const priced = rows.filter((r) => r.price !== null);
const out = {
  generated: new Date().toISOString().slice(0, 10),
  counts: {
    total: rows.length,
    priced: priced.length,
    whois: rows.filter((r) => r.whoisHost).length,
    rdap: rows.filter((r) => r.rdapServer).length,
    cctld: rows.filter((r) => r.kind === 'cctld').length,
    gtld: rows.filter((r) => r.kind === 'gtld').length,
    idn: rows.filter((r) => r.kind === 'idn').length,
    second: rows.filter((r) => r.level === 2).length,
    third: rows.filter((r) => r.level === 3).length,
    // The bare ccTLDs — what `--cctld --level second` leaves you with.
    rows2ndCctld: rows.filter((r) => r.kind === 'cctld' && r.level === 2).length,
    // Priced, but with no registry to ask — so outside what `--tld all` sweeps.
    none: rows.filter((r) => r.source === 'none').length,
  },
  priceRange: {
    min: Math.min(...priced.map((r) => r.price)),
    max: Math.max(...priced.map((r) => r.price)),
  },
  rows,
};

await writeFile(resolve(dataDir, 'tlds.json'), JSON.stringify(out));
const { total, priced: np, cctld: nc, gtld: ng, idn: ni } = out.counts;
console.log(`  tlds     ${total} rows — ${np} priced; ${ng} gTLD, ${nc} ccTLD, ${ni} IDN`);
