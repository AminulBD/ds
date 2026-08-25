// Builds src/data/tlds.json: one row per TLD, unioned from the sources `ds`
// itself consults — the bundled whois.json, pricing.json, private-tlds.json and
// eligibility.json, plus IANA's RDAP bootstrap. Run by `npm run gen` (and so by
// predev/prebuild).

import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath, domainToASCII, domainToUnicode } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
const dataDir = resolve(here, '../src/data');
const BOOTSTRAP_URL = 'https://data.iana.org/rdap/dns.json';
const FALLBACK = resolve(dataDir, 'rdap-dns.json');

/**
 * `.CO.UK` / `co.uk` / `.com` -> `co.uk` / `com`, as normalize_tld in
 * src/tlds.rs does, plus the punycode the CLI applies to every name it looks
 * up: whois.json spells one TLD `.বাংলা` where pricing.json and
 * eligibility.json spell it `xn--54b7fta0cc`, and without this the two halves
 * land on two rows that each know half the story.
 */
const normalize = (s) => {
  const bare = s.trim().replace(/^\.+|\.+$/g, '').toLowerCase();
  return domainToASCII(bare) || bare;
};

/**
 * Where a registrar's search lives, mirroring SEARCH_PAGES in src/registration.rs
 * so the site links a name to the same page `ds ... --where` would. `{domain}`
 * is filled in by the visitor's own name on the page; a registrar with no entry
 * is still named, and links to its own site.
 */
const SEARCH_PAGES = {
  'namecheap.com': 'https://www.namecheap.com/domains/registration/results/?domain={domain}',
  'porkbun.com': 'https://porkbun.com/checkout/search?q={domain}',
  'dynadot.com': 'https://www.dynadot.com/domain/search?domain={domain}',
  'namesilo.com': 'https://www.namesilo.com/domain/search-domains?query={domain}',
};

/** A registrar named by hostname gets a link home; a trading name gets none. */
const homepage = (registrar) =>
  /^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(registrar) ? `https://${registrar}/` : null;

/**
 * Look a TLD up longest suffix first — `com.au`, then `au` — exactly as
 * lookup_suffix in src/tlds.rs does, so a sub-zone inherits the rule above it.
 * Returns the entry and the TLD it was found under.
 */
function lookupSuffix(tld, table) {
  let rest = tld;
  for (;;) {
    const hit = table.get(rest);
    if (hit) return { hit, from: rest };
    const dot = rest.indexOf('.');
    if (dot < 0) return null;
    rest = rest.slice(dot + 1);
  }
}

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

/**
 * The place a two-letter TLD's code is assigned to, from ICU's own ISO 3166-1
 * table — `de` -> Germany. It names what the code stands for and nothing more:
 * a registry's actual rule about who may register lives in eligibility.json,
 * and plenty of ccTLDs (`.io`, `.tv`, `.co`) sell worldwide regardless.
 *
 * Codes ISO has since withdrawn are the trap: ICU quietly answers with the
 * successor state, so `.su` — still a live TLD — would come back "Russia".
 * Those are named here by hand instead.
 */
const REGION_NAMES = new Intl.DisplayNames(['en'], { type: 'region', fallback: 'none' });
const REGION_OVERRIDE = { su: 'the former Soviet Union' };
function region(tld) {
  const apex = tld.split('.').pop();
  if ([...apex].length !== 2) return null;
  if (apex in REGION_OVERRIDE) return REGION_OVERRIDE[apex];
  try {
    return REGION_NAMES.of(apex.toUpperCase()) ?? null;
  } catch {
    return null;
  }
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
    // `available` is the needle ds matches a WHOIS reply against, and
    // `comment` is the caveat the table carries where one exists — both are
    // how a WHOIS answer is reached, so the per-TLD page shows them.
    if (tld) whois.set(tld, { host, kind, needle: entry.available ?? null, comment: entry.comment ?? null });
  }
}

// --- pricing.json: one offer per registrar per TLD; the column is their mean ---
const pricing = new Map();
for (const [key, offers] of Object.entries(await readJson(resolve(repo, 'pricing.json')))) {
  const tld = normalize(key);
  if (!tld) continue;
  const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? Math.round(v * 100) / 100 : null);
  const mean = (field) => {
    const xs = offers
      .map((o) => o?.prices?.[field])
      .filter((v) => typeof v === 'number' && Number.isFinite(v));
    return xs.length ? Math.round((xs.reduce((a, b) => a + b, 0) / xs.length) * 100) / 100 : null;
  };
  pricing.set(tld, {
    price: mean('regular'),
    // Every quote in the file, mean and all — the table needs one figure per
    // TLD, the TLD's own page shows the three prices each registrar published,
    // including the ones with no registration price behind the average.
    offers: offers
      .map((o) => ({
        registrar: String(o?.register ?? '').trim().toLowerCase(),
        register: num(o?.prices?.regular),
        renew: num(o?.prices?.renew),
        transfer: num(o?.prices?.transfer),
      }))
      .filter((o) => o.registrar)
      .sort((a, b) => (a.register ?? Infinity) - (b.register ?? Infinity) || a.registrar.localeCompare(b.registrar))
      .map((o) => ({ ...o, search: SEARCH_PAGES[o.registrar] ?? null, home: homepage(o.registrar) })),
    renew: mean('renew'),
    transfer: mean('transfer'),
    // Whoever quoted a *registration* price, which is what the column shows —
    // the same list `--where` reads to say who sells the TLD, cheapest first.
    // A registrar that only published a renewal is in the file but behind
    // neither the figure nor the "register at" column. Mirrors summarise() in
    // src/pricing.rs, negative prices and anonymous quotes included.
    sellers: offers
      .filter((o) => typeof o?.prices?.regular === 'number' && Number.isFinite(o.prices.regular) && o.prices.regular >= 0)
      .map((o) => ({ registrar: String(o.register ?? '').trim().toLowerCase(), price: Math.round(o.prices.regular * 100) / 100 }))
      .filter((o) => o.registrar)
      .sort((a, b) => a.price - b.price || a.registrar.localeCompare(b.registrar))
      .map((o) => ({ ...o, search: SEARCH_PAGES[o.registrar] ?? null, home: homepage(o.registrar) })),
  });
}

// --- private-tlds.json: zones with no public registrations at all ---
// Keyed on the last label, exactly as the CLI's lookup is: an entry covers the
// whole TLD, so anything under it is equally closed.
const priv = new Map();
const privFile = await readJson(resolve(repo, 'private-tlds.json'));
for (const e of privFile.tlds) {
  priv.set(normalize(e.tld), { kind: e.kind, operator: e.operator ?? null });
}

// --- eligibility.json: who a registry will actually sell to ---
// Hand-maintained, unlike every other table here, and read longest suffix
// first so com.au inherits .au's Australian presence rule. See src/registration.rs.
const eligibility = new Map();
const eligFile = await readJson(resolve(repo, 'eligibility.json'));
for (const [tld, rule] of Object.entries(eligFile.tlds)) {
  eligibility.set(normalize(tld), rule);
}
// "Sources last checked 2026-08-25." from the file's own preamble, so the page
// dates the rules by when they were verified rather than by when it was built.
const eligChecked =
  [eligFile._about ?? []].flat().join(' ').match(/last checked (\d{4}-\d{2}-\d{2})/)?.[1] ?? null;

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
    const v = priv.get(tld.split('.').pop());
    return {
      tld,
      // How many labels the registration sits under: `com` = 2nd level,
      // `co.uk` = 3rd. Matches the --level filter in the CLI.
      level: tld.includes('.') ? 3 : 2,
      kind: classify(tld),
      // The readable form of a punycode TLD: xn--p1ai -> рф. null otherwise.
      unicode: classify(tld) === 'idn' ? domainToUnicode(tld) : null,
      cctld: classify(tld) === 'cctld',
      // The last label — `uk` for co.uk. What IANA files the zone under, and
      // what the type is measured on.
      apex: tld.split('.').pop(),
      // What ISO 3166-1 assigns the code to, for two-letter TLDs only. Not a
      // statement about who may register — see eligibility.
      region: region(tld),
      // 'brand' | 'infrastructure' | 'reserved', or null where nothing says the
      // zone is closed — which is not a claim that it is open. See src/private.rs.
      private: v?.kind ?? null,
      privateOperator: v?.operator ?? null,
      price: p.price ?? null,
      renew: p.renew ?? null,
      transfer: p.transfer ?? null,
      // The registrars whose published price proves they sell this TLD,
      // cheapest first — what `--where` prints as `register at`. Empty means
      // nobody in the table quotes it, not that nobody sells it.
      sellers: p.sellers ?? [],
      // Every published quote for the TLD, cheapest registration first, with
      // the renewal and transfer prices behind the means above.
      offers: p.offers ?? [],
      // The registry's own rule about who may register, plus `from`: the TLD
      // the rule was found under, which for com.au is au. null where the
      // hand-maintained list has no entry — which is not a claim of openness.
      eligibility: (() => {
        const e = lookupSuffix(tld, eligibility);
        return e ? { note: e.hit.note, source: e.hit.source, from: e.from } : null;
      })(),
      // Mirrors the real fall-through: RDAP first, WHOIS second.
      source: r && w ? 'both' : r ? 'rdap' : w ? 'whois' : 'none',
      rdapServer: r ?? null,
      whoisHost: w?.host ?? null,
      whoisKind: w?.kind ?? null,
      // The string a WHOIS reply must contain for ds to call the name free,
      // and the caveat whois.json attaches to the zone where it has one.
      whoisNeedle: w?.needle ?? null,
      whoisComment: w?.comment ?? null,
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
    // What `--tld all` leaves out unless you pass --private include.
    private: rows.filter((r) => r.private).length,
    second: rows.filter((r) => r.level === 2).length,
    third: rows.filter((r) => r.level === 3).length,
    // The bare ccTLDs — what `--cctld --level second` leaves you with.
    rows2ndCctld: rows.filter((r) => r.kind === 'cctld' && r.level === 2).length,
    // Priced, but with no registry to ask — so outside what `--tld all` sweeps.
    none: rows.filter((r) => r.source === 'none').length,
    // TLDs carrying an eligibility rule, their own or one inherited from above.
    restricted: rows.filter((r) => r.eligibility).length,
    // TLDs nobody in the price table sells, so `--where` has no one to name.
    unsold: rows.filter((r) => r.sellers.length === 0).length,
  },
  // Registrar -> how many TLDs it quotes a registration price for.
  sellers: Object.fromEntries(
    [...rows.flatMap((r) => r.sellers.map((s) => s.registrar))]
      .sort()
      .reduce((m, name) => m.set(name, (m.get(name) ?? 0) + 1), new Map()),
  ),
  // When the eligibility notes were last checked against the registry pages.
  eligibilityChecked: eligChecked,
  priceRange: {
    min: Math.min(...priced.map((r) => r.price)),
    max: Math.max(...priced.map((r) => r.price)),
  },
  rows,
};

await writeFile(resolve(dataDir, 'tlds.json'), JSON.stringify(out));
const { total, priced: np, cctld: nc, gtld: ng, idn: ni, private: nv, restricted: nr } = out.counts;
console.log(
  `  tlds     ${total} rows — ${np} priced; ${ng} gTLD, ${nc} ccTLD, ${ni} IDN; ${nv} private, ${nr} restricted`,
);
console.log(
  `  sellers  ${Object.entries(out.sellers).map(([r, n]) => `${r} ${n}`).join(', ')}`,
);
