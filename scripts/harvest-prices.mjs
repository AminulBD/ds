#!/usr/bin/env node
//
// Rebuilds pricing.json from the registrars that publish a price list.
//
//   node scripts/harvest-prices.mjs                     # every source below
//   node scripts/harvest-prices.mjs --sources porkbun   # just one
//   node scripts/harvest-prices.mjs --dry-run           # fetch, report, write nothing
//
// WHAT THIS DOES NOT DO
//
// The issue this script answers asked for prices "from all supported
// registry". That is not obtainable: registries sell wholesale to accredited
// registrars under contract, and outside the ICANN gTLD agreements (whose
// fees are per-registry PDFs, not a feed) they do not publish a retail price
// at all. There is no price a registry charges *you*. So this harvests
// registrars — the people who actually sell you the name — and the table is
// the mean over the registrars that quote each TLD.
//
// SOURCES
//
//   porkbun    https://api.porkbun.com/api/json/v3/pricing/get
//              Public, documented, no key, JSON, USD. One request.
//              `registration`/`renewal`/`transfer` are Porkbun's standing
//              list prices; the `coupons` array (time-limited discount codes)
//              is deliberately ignored.
//
//   101domain  https://www.101domain.com/<tld>.htm, enumerated from their
//              sitemap.xml. Each page carries a "Technical information"
//              table with Registration / Renewal / Transfer in USD. HTML, so
//              more brittle than an API, but it is the only broad source
//              found that covers the ccTLD long tail (.cn, .nu, .sm, ...)
//              and quotes in USD. robots.txt allows it; requests are
//              rate-limited and carry an honest User-Agent.
//
//   get.bd     https://get.bd/pricing.php
//              A BTCL-accredited registrar, and the only source found that
//              prices the .bd family at all — the second-level zones
//              (.com.bd, .org.bd, ...) that a Bangladeshi actually registers
//              are absent from every USD source. Server-rendered HTML; the
//              page answers 406 to a plain request and needs a browser
//              User-Agent and Accept header, so it gets one. The page states
//              the prices are BTCL's, not the registrar's own markup.
//
//              It quotes BDT, so it is the one source that is converted —
//              see PRICES, HONESTLY below.
//
//   namecheap  NOT harvested. Namecheap's price list page answers 403 to
//              anything without a browser fingerprint, and their pricing API
//              needs an account key plus an IP allowlist. The namecheap.com
//              offers already in pricing.json are carried through untouched;
//              they are a snapshot of an earlier manual pull and this script
//              never invents or edits them. To refresh them, replace them by
//              hand or drop them.
//
// PRICES, HONESTLY
//
//   * Every source above quotes USD except get.bd, which quotes BDT. That one
//     is converted, because the alternative is that ds prices .bd off a single
//     foreign reseller at eight times the registry's own list price and prices
//     .com.bd not at all. Conversion is not silent: each converted offer keeps
//     the figure the source published, the rate, and the date, in a `quoted`
//     block beside the USD `prices`, so a reader can tell a derived number
//     from a quoted one and re-derive the original. The rate is fetched at run
//     time from a named source and the run fails rather than fall back to a
//     stale constant. A converted price still drifts between harvests; the
//     `quoted.as_of` date is what says how far.
//   * `prices.regular` is the registrar's published *first-year* list price,
//     which at every registrar can sit well below the renewal price (.site is
//     a couple of dollars to register and forty-odd to renew). It is the
//     standing shelf price, not a coupon: Porkbun's coupon codes are dropped,
//     and no limited-time banner price is scraped. `prices.renew` is carried
//     alongside precisely so the gap is visible.
//   * A TLD is kept only if its last label is in the IANA root zone. That
//     drops the ~270 Handshake names Porkbun's feed carries, which are not
//     TLDs that ds can look up.
//   * One offer per registrar per TLD. The `register` field is what makes the
//     mean honest, so offers are never collapsed into a blended number.

import { readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { domainToASCII, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..');

const UA = 'ds-pricing-harvest/1.0 (+https://github.com/AminulBD/ds; domain price table for the ds CLI)';
const ROOT_ZONE_URL = 'https://data.iana.org/TLD/tlds-alpha-by-domain.txt';
const PORKBUN_URL = 'https://api.porkbun.com/api/json/v3/pricing/get';
const D101_SITEMAP = 'https://www.101domain.com/sitemap.xml';
const D101_PAGE = (tld) => `https://www.101domain.com/${tld}.htm`;
const GETBD_URL = 'https://get.bd/pricing.php';
// exchangerate-api's free endpoint: no key, dated, and it names its provider
// in the response. Fetched per run so no rate is ever baked into the repo.
const FX_URL = 'https://open.er-api.com/v6/latest/USD';

// get.bd answers 406 to anything that does not look like a browser. This is
// not a paywall or a robots.txt refusal — the page is public and linked from
// their nav — so it gets browser headers rather than being dropped.
const BROWSER_HEADERS = {
  'user-agent':
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36',
  accept: 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8',
  'accept-language': 'en-US,en;q=0.9',
};

// --- options ---------------------------------------------------------------

const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i === -1 ? fallback : args[i + 1];
};
const has = (name) => args.includes(`--${name}`);

const opts = {
  sources: String(flag('sources', 'porkbun,101domain,getbd')).split(',').map((s) => s.trim()).filter(Boolean),
  out: resolve(repo, flag('out', 'pricing.json')),
  // Milliseconds between the *start* of one 101domain request and the next.
  // ~2.5 req/s over ~1,300 pages: a few minutes, and gentle on the host.
  delay: Number(flag('delay', 400)),
  concurrency: Number(flag('concurrency', 3)),
  dryRun: has('dry-run'),
};

if (has('help')) {
  console.log(`usage: node scripts/harvest-prices.mjs [--sources porkbun,101domain,getbd]
       [--out pricing.json] [--delay ms] [--concurrency n] [--dry-run]`);
  process.exit(0);
}

// --- helpers ---------------------------------------------------------------

/** `.CO.UK` / `co.uk` -> `co.uk`. Mirrors normalize_tld in src/tlds.rs. */
const normalize = (s) => String(s).trim().replace(/^\.+|\.+$/g, '').toLowerCase();

/** A price is a quote only if it parses to a finite, non-negative number. */
const money = (v) => {
  if (v === null || v === undefined) return null;
  const n = Number(String(v).replace(/[$,\s]/g, ''));
  return Number.isFinite(n) && n >= 0 ? Math.round(n * 100) / 100 : null;
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function get(url, { as = 'text', retries = 2, headers } = {}) {
  for (let attempt = 0; ; attempt++) {
    try {
      const res = await fetch(url, {
        headers: headers ?? {
          'user-agent': UA,
          accept: as === 'json' ? 'application/json' : 'text/html,*/*',
        },
        signal: AbortSignal.timeout(30_000),
      });
      if (res.status === 404) return null; // A page that is not there is an answer.
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return as === 'json' ? await res.json() : await res.text();
    } catch (err) {
      if (attempt >= retries) throw new Error(`${url}: ${err.message}`);
      await sleep(1000 * 2 ** attempt); // Back off rather than retry straight away.
    }
  }
}

/**
 * Run `worker` over `items`, at most `concurrency` in flight and never more
 * than one request started per `delay` ms.
 */
async function crawl(items, worker, { concurrency, delay, onProgress }) {
  const queue = [...items];
  let done = 0;
  let nextSlot = 0;

  const run = async () => {
    for (;;) {
      const item = queue.shift();
      if (item === undefined) return;
      // Claim a slot before yielding, so two workers cannot both decide the
      // coast is clear and fire at once.
      const slot = Math.max(Date.now(), nextSlot);
      nextSlot = slot + delay;
      const wait = slot - Date.now();
      if (wait > 0) await sleep(wait);
      await worker(item);
      onProgress?.(++done, items.length);
    }
  };

  await Promise.all(Array.from({ length: Math.max(1, concurrency) }, run));
}

/** Last label of a suffix: `co.uk` -> `uk`. */
const apex = (tld) => tld.split('.').pop();

async function rootZone() {
  const text = await get(ROOT_ZONE_URL);
  const set = new Set(
    text
      .split('\n')
      .map((l) => l.trim().toLowerCase())
      .filter((l) => l && !l.startsWith('#')),
  );
  console.log(`  root     ${set.size} TLDs in the IANA root zone`);
  return set;
}

// --- sources ---------------------------------------------------------------

/**
 * Porkbun's public pricing endpoint. No key, one request, everything at once.
 * Their feed also carries Handshake names; the root-zone filter drops those.
 */
async function porkbun(root) {
  const json = await get(PORKBUN_URL, { as: 'json' });
  if (json?.status !== 'SUCCESS' || !json.pricing) throw new Error('porkbun: unexpected response shape');

  const offers = new Map();
  let skipped = 0;
  for (const [key, p] of Object.entries(json.pricing)) {
    const tld = normalize(key);
    if (!tld) continue;
    if (!root.has(apex(tld))) {
      skipped++;
      continue;
    }
    const regular = money(p.registration);
    if (regular === null) continue; // Nothing to average without a registration price.
    offers.set(tld, {
      register: 'porkbun.com',
      prices: { regular, renew: money(p.renewal), transfer: money(p.transfer) },
    });
  }
  console.log(`  porkbun  ${offers.size} TLDs priced (${skipped} non-root names skipped)`);
  return offers;
}

/**
 * 101domain publishes one page per TLD. Pull the candidates out of their
 * sitemap rather than guessing URLs, then read the "Technical information"
 * table off each page. Brand TLDs (.aarp, .abbott) have a page but no price,
 * and simply produce no offer.
 */
async function d101domain(root) {
  const xml = await get(D101_SITEMAP);
  const candidates = [
    ...new Set(
      [...xml.matchAll(/<loc>\s*([^<\s]+)\s*<\/loc>/g)]
        .map((m) => m[1].match(/^https:\/\/www\.101domain\.com\/([a-z0-9][a-z0-9.-]*)\.htm$/)?.[1])
        .filter((t) => t && root.has(apex(t))),
    ),
  ].sort();

  console.log(`  101dom   ${candidates.length} candidate pages from the sitemap`);

  const offers = new Map();
  let failed = 0;
  await crawl(
    candidates,
    async (tld) => {
      let html;
      try {
        html = await get(D101_PAGE(tld), { retries: 1 });
      } catch (err) {
        failed++;
        console.warn(`           WARNING: ${err.message}`);
        return;
      }
      if (!html) return;
      const parsed = parse101(html, tld);
      if (parsed) offers.set(tld, parsed);
    },
    {
      concurrency: opts.concurrency,
      delay: opts.delay,
      onProgress: (n, total) => {
        if (n % 100 === 0 || n === total) console.log(`           ${n}/${total} pages, ${offers.size} priced`);
      },
    },
  );

  console.log(`  101dom   ${offers.size} TLDs priced${failed ? ` (${failed} pages failed)` : ''}`);
  return offers;
}

/**
 * The page's technical-information list, e.g.
 *
 *   <span class="col-data-simple__heading">Registration</span>
 *   <span class="col-data-simple__text"> 14.99 USD / year
 *
 * Note the markup does not reliably close that second span, so each row is
 * read up to the next `<li`. Only USD figures are taken: if the page ever
 * starts quoting another currency the row is dropped rather than mixed in.
 */
function parse101(html, expected) {
  const rows = new Map();
  for (const chunk of html.split(/<li\b/)) {
    const heading = chunk.match(/class="col-data-simple__heading"[^>]*>\s*([^<]*?)\s*</)?.[1];
    if (!heading) continue;
    const text = chunk.match(/class="col-data-simple__text"[^>]*>([\s\S]*?)(?:<\/li>|$)/)?.[1];
    if (text !== undefined) rows.set(heading.trim().toLowerCase(), text);
  }

  // Guard against a marketing page that merely happens to be named like a
  // TLD (.contact, .support, ...): the page must say it is about this TLD.
  const claimed = rows.get('tld')?.match(/\.([a-z0-9.-]+)</)?.[1];
  if (normalize(claimed ?? '') !== expected) return null;

  const usd = (row) => money(rows.get(row)?.match(/([\d,]+\.\d{2})\s*USD/)?.[1]);
  const regular = usd('registration');
  if (regular === null) return null; // Brand TLDs, and anything not for sale.

  return {
    register: '101domain.com',
    prices: { regular, renew: usd('renewal'), transfer: usd('transfer') },
  };
}

/**
 * Today's USD rate for `code`, from a named, dated source.
 *
 * Deliberately has no fallback. A stale hardcoded rate would keep the harvest
 * green while quietly writing wrong prices, which is worse than not writing
 * them: if the rate cannot be had, the run stops and .bd keeps whatever it had
 * before.
 */
async function usdRate(code) {
  const json = await get(FX_URL, { as: 'json' });
  const rate = Number(json?.rates?.[code]);
  if (json?.result !== 'success' || !Number.isFinite(rate) || rate <= 0) {
    throw new Error(`${FX_URL}: no usable ${code} rate in the response`);
  }
  // A rate off by an order of magnitude turns a $12 domain into $120 without
  // anything looking wrong, so bound it rather than trust the feed blindly.
  if (rate < 1 || rate > 100_000) throw new Error(`${FX_URL}: implausible USD->${code} rate ${rate}`);
  return {
    rate,
    // ISO, like the `generated` stamp in private-tlds.json — a date in a data
    // file should sort and parse, not just read.
    as_of: Number.isFinite(json.time_last_update_unix)
      ? new Date(json.time_last_update_unix * 1000).toISOString().slice(0, 10)
      : null,
    provider: json.provider ?? FX_URL,
  };
}

/**
 * get.bd's price list — one card per extension:
 *
 *   <h3 ...>.com.bd</h3>
 *   <span ...>৳805</span> <span ...>/1st yr</span>
 *   <p ...>Renew: ৳1,840/yr</p>
 *
 * The page publishes no transfer price for any extension, so none is recorded
 * rather than one being guessed from the registration price.
 */
async function getbd(root) {
  const [html, fx] = await Promise.all([
    get(GETBD_URL, { headers: BROWSER_HEADERS }),
    usdRate('BDT'),
  ]);
  console.log(`  get.bd   1 USD = ${fx.rate} BDT (${fx.provider}, ${fx.as_of ?? 'undated'})`);

  const offers = new Map();
  let skipped = 0;
  for (const card of html.split(/<h3\b/).slice(1)) {
    const heading = card.match(/^[^>]*>\s*(\.[^<\s]+)\s*</)?.[1];
    if (!heading) continue;
    // The page prints Unicode; pricing.json is keyed in punycode like the
    // eight other IDN TLDs already in it, so .বাংলা lands as xn--54b7fta0cc.
    const tld = normalize(domainToASCII(normalize(heading)) || heading);
    if (!tld || !root.has(apex(tld))) {
      skipped++;
      continue;
    }
    const taka = (re) => money(card.match(re)?.[1]?.replace(/,/g, ''));
    const regularBdt = taka(/৳\s*([\d,]+)\s*<\/span>/);
    const renewBdt = taka(/Renew:\s*৳\s*([\d,]+)/);
    if (regularBdt === null) continue;

    const usd = (bdt) => (bdt === null ? null : Math.round((bdt / fx.rate) * 100) / 100);
    offers.set(tld, {
      register: 'get.bd',
      prices: { regular: usd(regularBdt), renew: usd(renewBdt), transfer: null },
      // What the source actually said, so the conversion is checkable and a
      // re-harvest converts BDT again rather than USD-of-a-USD.
      quoted: {
        currency: 'BDT',
        regular: regularBdt,
        renew: renewBdt,
        rate: fx.rate,
        as_of: fx.as_of,
        source: GETBD_URL,
      },
    });
  }

  if (!offers.size) throw new Error('get.bd: no prices parsed — the page has most likely been reshaped');
  console.log(`  get.bd   ${offers.size} TLDs priced, converted from BDT${skipped ? ` (${skipped} headings skipped)` : ''}`);
  return offers;
}

const SOURCES = { porkbun, '101domain': d101domain, getbd };

// --- merge and write -------------------------------------------------------

const unknown = opts.sources.filter((s) => !(s in SOURCES));
if (unknown.length) {
  console.error(`unknown source(s): ${unknown.join(', ')} — known: ${Object.keys(SOURCES).join(', ')}`);
  process.exit(2);
}

const existing = JSON.parse(await readFile(opts.out, 'utf8'));
const root = await rootZone();

// Registrars this run is responsible for; their old offers are replaced
// wholesale so a TLD they stopped selling does not linger.
const harvested = new Map(); // registrar -> Map(tld -> offer)
for (const name of opts.sources) harvested.set(name, await SOURCES[name](root));

const owned = new Set([...harvested.values()].flatMap((m) => [...m.values()].map((o) => o.register)));

// A source that has half-failed — the site up but reshaped, a feed truncated —
// would otherwise replace its own good data with a fraction of it, and the run
// would look like a success. Refuse to write instead.
for (const registrar of owned) {
  const had = Object.values(existing).filter((offers) => offers?.some?.((o) => o?.register === registrar)).length;
  const has = [...harvested.values()].reduce(
    (n, m) => n + [...m.values()].filter((o) => o.register === registrar).length,
    0,
  );
  if (had && has < had / 2) {
    console.error(
      `${registrar}: harvested ${has} TLDs but the current table has ${had} — refusing to write. ` +
        `Check the source before re-running; the parser has most likely gone stale.`,
    );
    process.exit(1);
  }
}

/** tld -> Map(registrar -> offer), so a registrar can only quote once. */
const table = new Map();
const put = (tld, offer) => {
  if (!table.has(tld)) table.set(tld, new Map());
  table.get(tld).set(offer.register, offer);
};

for (const [key, offers] of Object.entries(existing)) {
  const tld = normalize(key);
  if (!tld || !Array.isArray(offers)) continue;
  for (const offer of offers) {
    if (offer?.register && !owned.has(offer.register)) put(tld, offer);
  }
}
for (const offers of harvested.values()) for (const [tld, offer] of offers) put(tld, offer);

// Drop nulls so the file stays readable, and sort everything for stable diffs.
// An offer with only a renewal price is kept: `ds` still averages it into the
// renewal figure, it just does not count as a registration quote.
const clean = (prices) => Object.fromEntries(Object.entries(prices).filter(([, v]) => typeof v === 'number'));
const out = {};
for (const tld of [...table.keys()].sort()) {
  const offers = [...table.get(tld).values()]
    .sort((a, b) => a.register.localeCompare(b.register))
    // `quoted` rides along untouched, both for offers harvested this run and
    // for ones carried over from a source this run did not touch — dropping it
    // would strip the provenance off every converted price the moment someone
    // re-harvested porkbun alone.
    .map((o) => ({ register: o.register, prices: clean(o.prices), ...(o.quoted ? { quoted: o.quoted } : {}) }))
    .filter((o) => Object.keys(o.prices).length);
  if (offers.length) out[tld] = offers;
}

// --- report ----------------------------------------------------------------

// What `ds` will actually show: a TLD needs a *registration* price to get a
// figure in the price column, so count those rather than raw keys.
const withRegular = (table) =>
  Object.values(table).filter((offers) => offers.some((o) => typeof o?.prices?.regular === 'number')).length;

const countBy = (fn) => Object.values(out).reduce((n, offers) => n + (fn(offers) ? 1 : 0), 0);
const perRegistrar = new Map();
for (const offers of Object.values(out)) {
  for (const o of offers) perRegistrar.set(o.register, (perRegistrar.get(o.register) ?? 0) + 1);
}

console.log('');
console.log(`  priced   ${withRegular(existing)} TLDs before, ${withRegular(out)} after`);
for (const [registrar, n] of [...perRegistrar].sort((a, b) => b[1] - a[1])) {
  console.log(`           ${String(n).padStart(5)}  ${registrar}`);
}
console.log(
  `           ${countBy((o) => o.filter((x) => typeof x.prices.regular === 'number').length > 1)} TLDs quoted by more than one registrar`,
);

if (opts.dryRun) {
  console.log('\n  --dry-run: nothing written');
} else {
  await writeFile(opts.out, JSON.stringify(out, null, 2) + '\n');
  console.log(`\n  wrote    ${opts.out}`);
}
