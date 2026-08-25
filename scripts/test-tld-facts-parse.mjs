#!/usr/bin/env node
//
// Checks the parser in scripts/harvest-tld-facts.mjs against a trimmed copy of
// IANA's Root Zone Database, offline.
//
//     node scripts/test-tld-facts-parse.mjs
//
// The harvester reads one HTML page, which is the brittle part: IANA can
// reshape that table at any time, and a parser that quietly returns nothing
// would take tld-facts.json with it. The size guard in the script catches a total
// failure; these pin the shape it is guarding.

import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { CATEGORY_RULES, categorize, parseRootDb } from './harvest-tld-facts.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const failures = [];
let checks = 0;

const check = (name, got, want) => {
  checks++;
  const a = JSON.stringify(got);
  const b = JSON.stringify(want);
  if (a !== b) failures.push(`${name}: expected ${b}, got ${a}`);
};

const db = parseRootDb(await readFile(resolve(here, 'fixtures/iana-root-db.html'), 'utf8'));

// --- the table -------------------------------------------------------------

check('only TLD rows are parsed', db.size, 7);
check('the page\'s own navigation is not a TLD', [...db.keys()].some((k) => k.includes('domains')), false);
check('a legacy gTLD', db.get('com'), {
  type: 'generic', sponsor: 'VeriSign Global Registry Services', assigned: true,
});
check('a ccTLD', db.get('uk').type, 'country-code');
check('the infrastructure TLD', db.get('arpa').type, 'infrastructure');
check('a sponsored TLD', db.get('museum').type, 'sponsored');

// The href carries punycode and the link text the unicode label, which is the
// only place the readable form of an IDN appears on the page.
check('an IDN is keyed in punycode', db.has('xn--11b4c3d'), true);
check('...and keeps its unicode label', db.get('xn--11b4c3d').unicode, 'कॉम');
check('...without a leading dot', db.get('xn--11b4c3d').unicode.startsWith('.'), false);
check('a latin TLD carries no unicode label', 'unicode' in db.get('com'), false);

// "Not assigned" is IANA saying there is no operator — not the operator's name.
check('an unassigned TLD records no sponsor', db.get('abarth').sponsor, null);
check('...and says so', db.get('abarth').assigned, false);

check('entities in a sponsor name are decoded', db.get('aco').sponsor, 'ACO Severin Ahlmann GmbH & Co. KG');

// --- categories ------------------------------------------------------------

const cat = (tld, icann, inRootZone = true) => categorize(tld, { rootDb: db.get(tld), icann, inRootZone });

check('a TLD with no ICANN record gets its IANA type', cat('uk', null), ['country-code']);
check('.com is a legacy gTLD, not a new one', cat('com', { delegationDate: '1985-01-01' }), ['generic', 'legacy-gtld']);
check('the 2013 round opens new-gtld', cat('com', { delegationDate: '2013-10-23' }), ['generic', 'new-gtld']);
check('the day before it does not', cat('com', { delegationDate: '2013-10-22' }), ['generic', 'legacy-gtld']);
check('punycode implies idn', cat('xn--11b4c3d', { delegationDate: '2015-07-28' }), ['generic', 'idn', 'new-gtld']);
check('Specification 13 is a brand', cat('com', { specification13: true, delegationDate: '2015-08-28' }), [
  'generic', 'brand', 'new-gtld',
]);
check('a TLD gone from the root zone says so', cat('abarth', {
  delegationDate: '2016-08-04', contractTerminated: true, removalDate: '2023-06-05',
}, false), ['generic', 'unassigned', 'removed-from-root', 'new-gtld', 'contract-terminated', 'removed']);
check('third-level registration is recorded', cat('museum', { thirdOrLowerLevelRegistration: true }), [
  'sponsored', 'third-level',
]);

// A category with nothing explaining it is a label with nothing behind it.
const everyCategory = new Set([
  ...cat('com', { specification13: true, delegationDate: '2015-01-01', contractTerminated: true, removalDate: '2020-01-01', thirdOrLowerLevelRegistration: true }),
  ...cat('abarth', { delegationDate: '2001-01-01' }, false),
  ...cat('xn--11b4c3d', null),
  ...cat('uk', null),
  ...cat('arpa', null),
  ...cat('museum', null),
]);
check('every category emitted is explained', [...everyCategory].filter((c) => !(c in CATEGORY_RULES)), []);
check('an unexplained type is not emitted', categorize('x', { rootDb: { type: 'invented', assigned: true }, inRootZone: true }), []);

// --- report ----------------------------------------------------------------

if (failures.length) {
  for (const f of failures) console.log(`FAIL  ${f}`);
  console.log(`\n${failures.length}/${checks} failed`);
  process.exit(1);
}
console.log(`ok — ${checks} checks against scripts/fixtures/iana-root-db.html`);
