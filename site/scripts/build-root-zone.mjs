// Builds src/data/root-zone.json from the four files IANA publishes at
// https://www.iana.org/domains/root/files — the root zone itself, the root
// hints, the DNSSEC trust anchors, and the list of delegated TLDs.
//
// The root zone is the authority on two things nothing else in this repo
// knows. First, whether a TLD is signed: the root publishes a DS record for
// each signed zone, and 88 TLDs have none. Second, what is actually delegated
// today — a name in pricing.json that the root has never heard of cannot
// resolve, whoever is selling it.
//
// Run by `npm run gen` (and so by predev/prebuild), before build-tlds.mjs,
// which joins the per-TLD half of this file onto its rows.
//
// The output is gitignored: it is ~700 KB and the root's serial moves most
// days, so committing it would put a rewritten blob of it in the history of
// every site change. It is fetched on each build instead — including in the
// Pages workflow — so what deploys is the root as it stood that morning. A
// copy left in the working tree by an earlier build is used only when a fetch
// fails locally; with no copy and no answer from IANA the build stops rather
// than publishing a site with the section quietly missing.
//
// Locally the file is left in place and reused for a day, the way src/tlds.rs
// caches the RDAP bootstrap for a week: a `npm run dev` should not pull two
// megabytes off InterNIC every time it restarts, and the root changes about
// once a day. `--force` refetches now. CI has no file to reuse, so a deploy
// always fetches.
//
// Each TLD and each name server is written on one line — the file is 1,400
// rows of data, not a document, and one line per record keeps it readable if
// you open it, without running to 30,000 lines.

import { readFile, writeFile, mkdir, stat } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const dataDir = resolve(here, '../src/data');
const OUT = resolve(dataDir, 'root-zone.json');

/** How long a local copy is good for. The root zone is cut once a day. */
const MAX_AGE = 24 * 60 * 60 * 1000;
const FORCE = process.argv.includes('--force');
/** The keys a usable file has to carry, so a copy from an older version of
 *  this script is refetched rather than half-read by the pages. */
const SHAPE = ['generated', 'sources', 'zone', 'hints', 'anchors', 'list', 'counts', 'nameservers', 'tlds'];

const SOURCES = {
  // The zone itself. ~2 MB of text, and the only one of the four that carries
  // per-TLD records; everything under `tlds` and `nameservers` comes from here.
  zone: 'https://www.internic.net/domain/root.zone',
  // The root servers and their addresses, with the operator named in the
  // comments above each one.
  hints: 'https://www.internic.net/domain/named.root',
  // The keys a validating resolver is configured with — the top of every
  // DNSSEC chain, and what the root's own KSK has to match.
  anchors: 'https://data.iana.org/root-anchors/root-anchors.xml',
  // IANA's plain-text list of delegated TLDs, carrying the serial it was cut
  // from. A second opinion on the zone's own delegation count.
  list: 'https://data.iana.org/TLD/tlds-alpha-by-domain.txt',
};

/** DNSSEC algorithm numbers, from the IANA DNSSEC Algorithm Numbers registry. */
const ALGORITHMS = {
  1: 'RSA/MD5',
  3: 'DSA/SHA-1',
  5: 'RSA/SHA-1',
  6: 'DSA-NSEC3-SHA1',
  7: 'RSASHA1-NSEC3-SHA1',
  8: 'RSA/SHA-256',
  10: 'RSA/SHA-512',
  12: 'GOST R 34.10-2001',
  13: 'ECDSA P-256/SHA-256',
  14: 'ECDSA P-384/SHA-384',
  15: 'Ed25519',
  16: 'Ed448',
};

/** DS digest algorithms, from the IANA Delegation Signer Digest registry. */
const DIGESTS = { 1: 'SHA-1', 2: 'SHA-256', 3: 'GOST R 34.11-94', 4: 'SHA-384' };

/**
 * Fetch one source as text, with one retry. Nothing here is cached between
 * builds, so a single dropped connection would otherwise be the difference
 * between a deploy and a red pipeline; a second attempt a few seconds later
 * costs one request and answers most of them.
 */
async function fetchText(url, timeoutMs) {
  let last;
  for (let attempt = 1; attempt <= 2; attempt++) {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return await res.text();
    } catch (err) {
      last = err;
      if (attempt < 2) await new Promise((r) => setTimeout(r, 3000));
    }
  }
  throw last;
}

/**
 * Whatever an earlier build in this working tree left behind. Gitignored, so
 * it exists on a machine that has built the site before and never in CI — it
 * is a convenience for working offline, not a shipped fallback.
 */
const previous = await (async () => {
  if (!existsSync(OUT)) return null;
  try {
    const json = JSON.parse(await readFile(OUT, 'utf8'));
    return SHAPE.every((k) => k in json) ? json : null;
  } catch {
    // A half-written file from an interrupted build is not a fallback.
    return null;
  }
})();

// Fresh enough to reuse as it stands. Measured on the file's own timestamp
// rather than its `generated` date, so a copy written five minutes ago counts
// as five minutes old even when the day has rolled over since.
if (previous && !FORCE) {
  const age = Date.now() - (await stat(OUT)).mtimeMs;
  if (age < MAX_AGE) {
    const hours = Math.floor(age / 3600000);
    console.log(
      `  root     serial ${previous.zone.serial} — ${previous.counts.delegated} TLDs, reusing the copy from ` +
        `${hours < 1 ? 'under an hour' : `${hours} hour${hours === 1 ? '' : 's'}`} ago (--force to refetch)`,
    );
    process.exit(0);
  }
}

/**
 * Fetch and parse one source, falling back to that section of the previous
 * build's output. Returns the parsed value and whether it came off the
 * network, so the page can say which parts of it are today's.
 */
async function section(name, parse, { timeout = 60000, keys }) {
  try {
    const value = parse(await fetchText(SOURCES[name], timeout));
    console.log(`  root     ${name.padEnd(8)} fetched`);
    return { value, live: true };
  } catch (err) {
    if (!previous) {
      throw new Error(
        `${name}: cannot fetch ${SOURCES[name]} (${err.message}), and no previous build left a copy at ${OUT}. ` +
          'The root zone is fetched fresh on every build — fix the network or the URL rather than committing a copy.',
      );
    }
    console.warn(
      `  root     ${name.padEnd(8)} WARNING: fetch failed (${err.message}) — reusing the previous build's copy, which may be stale`,
    );
    return { value: Object.fromEntries(keys.map((k) => [k, previous[k]])), live: false };
  }
}

/**
 * The RFC 4034 Appendix B key tag: the checksum a DS record names its key by.
 * Computed here so the page can say the KSK in the zone is the key the trust
 * anchor covers, rather than printing two numbers and hoping the reader
 * compares them.
 */
function keyTag(flags, protocol, algorithm, publicKey) {
  const rdata = Buffer.concat([
    Buffer.from([flags >> 8, flags & 0xff, protocol, algorithm]),
    Buffer.from(publicKey, 'base64'),
  ]);
  let ac = 0;
  for (const [i, b] of rdata.entries()) ac += i & 1 ? b : b << 8;
  ac += (ac >> 16) & 0xffff;
  return ac & 0xffff;
}

/**
 * `2001:503:a83e:0:0:0:2:30` -> `2001:503:a83e::2:30`. The zone writes every
 * group out in full; this is the same address in the form RFC 5952 calls
 * canonical and everyone else writes, with the longest run of zero groups —
 * two or more, leftmost wins a tie — replaced by `::`. IPv4 is returned
 * untouched. Checked against named.root, which spells the root servers'
 * addresses the short way, so a wrong answer here would show up as thirteen
 * mismatches rather than silently.
 */
function compress6(addr) {
  if (!addr.includes(':')) return addr;
  const groups = addr.toLowerCase().split(':').map((g) => g.replace(/^0+(?=.)/, ''));
  let best = { at: -1, len: 0 };
  for (let i = 0; i < groups.length; i++) {
    if (groups[i] !== '0') continue;
    let j = i;
    while (j < groups.length && groups[j] === '0') j++;
    if (j - i > best.len) best = { at: i, len: j - i };
    i = j - 1;
  }
  if (best.len < 2) return groups.join(':');
  const head = groups.slice(0, best.at).join(':');
  const tail = groups.slice(best.at + best.len).join(':');
  return `${head}::${tail}`;
}

/**
 * The root zone: one record per line, `name TTL IN TYPE rdata...`, all names
 * lowercase and fully qualified, nothing wrapped in parentheses. Only four
 * types matter here — the signatures, NSEC chain and ZONEMD are how the zone
 * proves itself to a resolver, not facts about a TLD.
 */
function parseZone(text) {
  const soa = {};
  const rootServers = [];
  const keys = [];
  const tlds = new Map();
  const nameservers = new Map();

  const entry = (tld) => {
    let e = tlds.get(tld);
    if (!e) tlds.set(tld, (e = { ns: [], ds: [] }));
    return e;
  };

  for (const line of text.split('\n')) {
    if (!line || line.startsWith(';')) continue;
    const f = line.split(/\s+/);
    // `.` -> '' (the root), `com.` -> 'com', `a.gtld-servers.net.` -> itself
    // without the trailing dot. Owner names below a TLD are glue, not TLDs.
    const owner = f[0].replace(/\.$/, '');
    const type = f[3];

    if (owner === '') {
      if (type === 'SOA') {
        const [primary, mail, serial, refresh, retry, expire, minimum] = f.slice(4);
        Object.assign(soa, {
          primary: primary.replace(/\.$/, ''),
          // The RNAME is an address with the @ written as a dot.
          hostmaster: mail.replace(/\.$/, '').replace('.', '@'),
          serial: Number(serial),
          ttl: Number(f[1]),
          refresh: Number(refresh),
          retry: Number(retry),
          expire: Number(expire),
          minimum: Number(minimum),
        });
      } else if (type === 'NS') {
        rootServers.push(f[4].replace(/\.$/, ''));
      } else if (type === 'DNSKEY') {
        const [flags, protocol, algorithm] = f.slice(4, 7).map(Number);
        keys.push({
          // Bit 0 of the flags is the Secure Entry Point: set on the
          // key-signing key the trust anchor covers, clear on the zone-signing
          // key that signs everything else and rolls every few months.
          kind: flags & 0x1 ? 'KSK' : 'ZSK',
          flags,
          algorithm,
          keyTag: keyTag(flags, protocol, algorithm, f.slice(7).join('')),
        });
      }
      continue;
    }

    if (type === 'NS') {
      entry(owner).ns.push(f[4].replace(/\.$/, ''));
    } else if (type === 'DS') {
      // [key tag, algorithm, digest type, digest] — the fingerprint of the
      // TLD's own key-signing key, which is what makes the zone signed.
      entry(owner).ds.push([Number(f[4]), Number(f[5]), Number(f[6]), f.slice(7).join('')]);
    } else if (type === 'A' || type === 'AAAA') {
      // Glue: the address of a name server that lives inside the zone it
      // serves, so a resolver can reach it without a chicken-and-egg lookup.
      // v4 and v6 share one list — a colon says which is which.
      const addrs = nameservers.get(owner) ?? [];
      addrs.push(compress6(f[4]));
      nameservers.set(owner, addrs);
    }
  }

  // Delegations only. Anything with glue but no NS record of its own is a name
  // server, not a TLD; anything with a DS but no NS should not exist.
  for (const [tld, e] of tlds) if (e.ns.length === 0) tlds.delete(tld);

  return {
    zone: { ...soa, servers: rootServers, keys },
    tlds: Object.fromEntries([...tlds].sort(([a], [b]) => a.localeCompare(b))),
    nameservers: Object.fromEntries([...nameservers].sort(([a], [b]) => a.localeCompare(b))),
  };
}

/**
 * named.root: the same record format, plus comments naming who runs each
 * server. Only twelve organisations run the thirteen; the file says so for
 * four of them and gives the rest's original hostname instead, which is the
 * only attribution the file carries, so both are kept as written.
 */
function parseHints(text) {
  const servers = [];
  const addrs = new Map();
  let note = null;

  for (const line of text.split('\n')) {
    if (line.startsWith(';')) {
      const c = line.slice(1).trim();
      if (/^OPERATED BY /i.test(c)) note = { operator: c.slice('OPERATED BY '.length) };
      else if (/^FORMERLY /i.test(c)) note = { formerly: c.slice('FORMERLY '.length).trim() };
      continue;
    }
    const f = line.trim().split(/\s+/);
    if (f.length < 4) continue;
    const [owner, , type, rdata] = f;
    const name = owner.replace(/\.$/, '').toLowerCase();
    if (type === 'NS') {
      servers.push({ host: rdata.replace(/\.$/, '').toLowerCase(), ...(note ?? {}) });
      note = null;
    } else if (type === 'A' || type === 'AAAA') {
      addrs.set(name, [...(addrs.get(name) ?? []), rdata]);
    }
  }

  return {
    hints: {
      // "last update:     July 29, 2026" and the root zone serial it matches.
      updated: text.match(/last update:\s*(.+)/)?.[1].trim() ?? null,
      serial: Number(text.match(/related version of root zone:\s*(\d+)/)?.[1]) || null,
      servers: servers.map((s) => ({
        ...s,
        v4: (addrs.get(s.host) ?? []).filter((a) => !a.includes(':')),
        v6: (addrs.get(s.host) ?? []).filter((a) => a.includes(':')),
      })),
    },
  };
}

/**
 * root-anchors.xml: a handful of KeyDigest elements, one per KSK IANA has ever
 * published, each with the window it is valid in. Read with a regex rather
 * than a dependency — the file is four elements of fixed shape.
 */
function parseAnchors(xml) {
  const anchors = [];
  for (const m of xml.matchAll(/<KeyDigest\b([^>]*)>([\s\S]*?)<\/KeyDigest>/g)) {
    const [, attrs, body] = m;
    const attr = (k) => attrs.match(new RegExp(`${k}="([^"]*)"`))?.[1] ?? null;
    const tag = (k) => body.match(new RegExp(`<${k}>([\\s\\S]*?)</${k}>`))?.[1].trim() ?? null;
    anchors.push({
      id: attr('id'),
      keyTag: Number(tag('KeyTag')),
      algorithm: Number(tag('Algorithm')),
      digestType: Number(tag('DigestType')),
      digest: tag('Digest'),
      // Absent on the anchor currently in force — it has no end date yet.
      validFrom: attr('validFrom')?.slice(0, 10) ?? null,
      validUntil: attr('validUntil')?.slice(0, 10) ?? null,
    });
  }
  if (!anchors.length) throw new Error('no KeyDigest elements');
  return { anchors };
}

/** tlds-alpha-by-domain.txt: a version comment, then one TLD per line. */
function parseList(text) {
  const lines = text.split('\n').map((l) => l.trim()).filter(Boolean);
  const header = lines[0].startsWith('#') ? lines.shift() : '';
  const names = lines.map((l) => l.toLowerCase());
  if (!names.length) throw new Error('no TLDs in the list');
  return {
    list: {
      version: header.match(/Version (\d+)/)?.[1] ?? null,
      updated: header.match(/Last Updated (.+)$/)?.[1] ?? null,
      count: names.length,
      tlds: names,
    },
  };
}

await mkdir(dataDir, { recursive: true });

const [zone, hints, anchors, list] = await Promise.all([
  section('zone', parseZone, { keys: ['zone', 'tlds', 'nameservers'] }),
  section('hints', parseHints, { timeout: 20000, keys: ['hints'] }),
  section('anchors', parseAnchors, { timeout: 20000, keys: ['anchors'] }),
  section('list', parseList, { timeout: 20000, keys: ['list'] }),
]);

const rootTlds = Object.entries(zone.value.tlds);
const glue = Object.values(zone.value.nameservers);
const listed = new Set(list.value.list.tlds);

// Which TLDs the two sources disagree about. They are cut from the same serial
// and normally agree exactly; when they do not, one of them is a day behind.
const zoneOnly = rootTlds.filter(([t]) => !listed.has(t)).map(([t]) => t);
const listOnly = list.value.list.tlds.filter((t) => !(t in zone.value.tlds));

const out = {
  generated: new Date().toISOString().slice(0, 10),
  // Where each half of this file came from, and whether it is today's or the
  // committed copy kept because the fetch failed.
  sources: Object.fromEntries(
    Object.entries(SOURCES).map(([name, url]) => [
      name,
      { url, live: { zone, hints, anchors, list }[name].live },
    ]),
  ),
  zone: zone.value.zone,
  hints: hints.value.hints,
  anchors: anchors.value.anchors,
  list: list.value.list,
  counts: {
    delegated: rootTlds.length,
    signed: rootTlds.filter(([, e]) => e.ds.length > 0).length,
    unsigned: rootTlds.filter(([, e]) => e.ds.length === 0).length,
    // NS records across the whole zone, and the servers they point at — far
    // fewer, because one machine answers for hundreds of TLDs.
    delegations: rootTlds.reduce((n, [, e]) => n + e.ns.length, 0),
    nameservers: Object.keys(zone.value.nameservers).length,
    glue4: glue.reduce((n, a) => n + a.filter((x) => !x.includes(':')).length, 0),
    glue6: glue.reduce((n, a) => n + a.filter((x) => x.includes(':')).length, 0),
    ds: rootTlds.reduce((n, [, e]) => n + e.ds.length, 0),
    // TLDs every one of whose name servers has a v6 address in the zone.
    v6: rootTlds.filter(([, e]) =>
      e.ns.length > 0 &&
      e.ns.every((h) => (zone.value.nameservers[h] ?? []).some((a) => a.includes(':'))),
    ).length,
  },
  // How many TLDs are signed with each algorithm — the shape of the root's
  // migration from RSA to the elliptic-curve ones.
  algorithmUse: Object.fromEntries(
    [...rootTlds.flatMap(([, e]) => e.ds.map((d) => d[1]))]
      .sort((a, b) => a - b)
      .reduce((m, a) => m.set(a, (m.get(a) ?? 0) + 1), new Map()),
  ),
  algorithms: ALGORITHMS,
  digests: DIGESTS,
  // Empty on a normal day. Non-empty means the zone and IANA's list were cut
  // at different moments, which is worth saying rather than silently picking one.
  disagree: { zoneOnly, listOnly },
  nameservers: zone.value.nameservers,
  tlds: zone.value.tlds,
};

/**
 * JSON.stringify with the leaves of `tlds` and `nameservers` kept on one line.
 * Pretty-printing them the usual way puts every name server and every DS field
 * on a line of its own — 30,000 lines for 1,400 TLDs — and minifying the file
 * makes its diff one unreadable line. Marked values are stringified compactly
 * into a placeholder string, then unwrapped afterwards.
 */
const MARK = '\u0001';
const compact = (v) => MARK + JSON.stringify(v);
// The marker survives stringify as the six-character escape \u0001, which
// nothing else in this data can produce, so the unwrap cannot hit a real string.
const emit = (obj) =>
  JSON.stringify(obj, null, 2).replace(/"\\u0001((?:[^"\\]|\\.)*)"/g, (_, s) => JSON.parse(`"${s}"`));

await writeFile(
  OUT,
  emit({
    ...out,
    nameservers: Object.fromEntries(Object.entries(out.nameservers).map(([k, v]) => [k, compact(v)])),
    tlds: Object.fromEntries(Object.entries(out.tlds).map(([k, v]) => [k, compact(v)])),
  }) + '\n',
);

const c = out.counts;
console.log(
  `  root     serial ${out.zone.serial} — ${c.delegated} TLDs delegated, ${c.signed} signed (${Math.round((c.signed / c.delegated) * 100)}%), ` +
    `${c.nameservers} name servers, ${c.glue4} A and ${c.glue6} AAAA glue records`,
);
if (zoneOnly.length || listOnly.length) {
  console.warn(
    `  root     WARNING: the zone and IANA's TLD list disagree — ` +
      `${zoneOnly.length} only in the zone (${zoneOnly.slice(0, 5).join(', ')}), ` +
      `${listOnly.length} only in the list (${listOnly.slice(0, 5).join(', ')})`,
  );
}
