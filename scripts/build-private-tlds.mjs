// Regenerates ../private-tlds.json — the TLDs where nobody outside the registry
// operator can register a name. Run with `node scripts/build-private-tlds.mjs`.
//
// Why ICANN's file and not IANA's root database: the root database is the
// obvious place to look, but its Type column only ever says generic /
// country-code / sponsored / infrastructure. It has no notion of a .brand, so
// picking brands out of it would mean reading 1,200 sponsoring-organisation
// names and guessing which ones are a company registering its own name — which
// is exactly the hand-curation this file exists to avoid.
//
// ICANN publishes the contractual fact instead. Every new gTLD signs a registry
// agreement, and a registry that operates its TLD purely for itself applies for
// Specification 13, the ".brand TLD" exemption: it lets the registry be its own
// sole registrant and drops the registrar non-discrimination rules that
// otherwise put a TLD on the open market. `specification13: true` is therefore
// a first-party, dated, checkable statement that the public cannot buy a name
// in that zone.
//
// The converse does not hold and this script does not pretend it does: a gTLD
// without Specification 13 is not thereby shown to be on sale, so nothing is
// recorded for it and `ds` makes no claim about it either way.

import { writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const OUT = resolve(dirname(fileURLToPath(import.meta.url)), '../private-tlds.json');
const ICANN = 'https://www.icann.org/resources/registries/gtlds/v2/gtlds.json';

// Names the IETF has reserved so that they can never be delegated to anyone.
// Short enough to write out, and each line carries the RFC that reserves it.
const IETF = [
  ['arpa', 'infrastructure', 'RFC 3172'],
  ['example', 'reserved', 'RFC 2606'],
  ['invalid', 'reserved', 'RFC 2606'],
  ['localhost', 'reserved', 'RFC 2606'],
  ['test', 'reserved', 'RFC 2606'],
  ['local', 'reserved', 'RFC 6762'],
  ['onion', 'reserved', 'RFC 7686'],
];

const res = await fetch(ICANN, { signal: AbortSignal.timeout(30000) });
if (!res.ok) throw new Error(`${ICANN}: HTTP ${res.status}`);
const { gTLDs, updatedOn } = await res.json();
if (!Array.isArray(gTLDs)) throw new Error(`${ICANN}: no gTLDs array`);

// A terminated or removed agreement means the TLD is on its way out of the
// root, so whatever it used to be it is not a zone anyone can be pointed at.
const brands = gTLDs
  .filter((g) => g.specification13 === true)
  .filter((g) => g.delegationDate && !g.contractTerminated && !g.removalDate)
  .map((g) => ({
    tld: g.gTLD.toLowerCase(),
    kind: 'brand',
    operator: g.registryOperator ?? null,
  }));

if (brands.length < 200) throw new Error(`only ${brands.length} brand TLDs — the feed looks wrong`);

const tlds = [...brands, ...IETF.map(([tld, kind, rfc]) => ({ tld, kind, rfc }))].sort((a, b) =>
  a.tld.localeCompare(b.tld),
);

const head = JSON.stringify(
  {
    generated: new Date().toISOString().slice(0, 10),
    sources: {
      brand: `${ICANN} (specification13), updated ${updatedOn.slice(0, 10)}`,
      infrastructure: 'RFC 3172',
      reserved: 'RFC 2606, RFC 6762, RFC 7686',
    },
  },
  null,
  2,
).replace(/\n}$/, '');

// One TLD per line: this file is read by a human reviewing a data change far
// more often than it is read by a machine.
const body = tlds.map((t) => `    ${JSON.stringify(t)}`).join(',\n');
await writeFile(OUT, `${head},\n  "tlds": [\n${body}\n  ]\n}\n`);

console.log(
  `private-tlds.json  ${tlds.length} entries — ${brands.length} brand, ${IETF.length} IETF-reserved`,
);
