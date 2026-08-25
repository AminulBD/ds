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

import { CATEGORY_RULES, categorize, parseDelegation, parseRootDb, topicIndex }
  from './harvest-tld-facts.mjs';

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

// --- delegation records (--deep) --------------------------------------------

const delegations = await readFile(resolve(here, 'fixtures/iana-delegation.html'), 'utf8');
const record = (tld) => parseDelegation(delegations.split(`<!-- .${tld} —`)[1]?.split('</section>')[0] ?? '');

const academy = record('academy');
check('a gTLD names its manager', academy.manager, 'Binky Moon, LLC');
check('...and the country from the last address line', academy.country, 'United States of America (the)');
check('...its registration URL', academy.url, 'https://www.identity.digital/');
check('...its RDAP service', academy.rdap, 'https://rdap.identitydigital.services/rdap/');
check('...its name servers, sorted and deduplicated', academy.nameservers, [
  'v0n0.nic.academy', 'v0n1.nic.academy', 'v0n2.nic.academy',
  'v0n3.nic.academy', 'v2n0.nic.academy', 'v2n1.nic.academy',
]);
check('...and both dates', [academy.registered, academy.updated], ['2013-12-12', '2025-10-07']);
// IANA lists no WHOIS server for .academy. A missing field must stay missing
// rather than becoming an empty string that reads like an answer.
check('a field IANA does not publish is absent', 'whois' in academy, false);

const aero = record('aero');
check('a WHOIS server is taken when there is one', aero.whois, 'whois.aero');
check('...lowercased, like whois.json keys them', aero.whois, aero.whois.toLowerCase());

// ccTLDs file the operator under a different heading entirely.
const bd = record('bd');
check('a ccTLD manager is found under its own heading', bd.manager, 'Posts and Telecommunications Division');
check('...with its country', bd.country, 'Bangladesh');
check('a registry with neither service gets neither field', ['whois' in bd, 'rdap' in bd], [false, false]);

// A removed delegation keeps its dates and loses everything else.
const abarth = record('abarth');
check('a removed delegation still parses', abarth, { registered: '2016-07-14', updated: '2023-06-05' });
check('a page with no record at all is null', parseDelegation('<p>nothing here</p>'), null);

// The contacts are the point of the exercise: they are never collected, and
// they are not in the fixture either.
check('no contact data is parsed', Object.keys(academy).some((k) => /email|phone|voice|fax|contact/i.test(k)), false);
check('no contact data is even in the fixture', /Voice:|Fax:|@[a-z0-9.-]+\.[a-z]{2,}/i.test(delegations), false);

// --- the hand-maintained taxonomy -------------------------------------------
//
// tld-categories.json is the one file here nobody generates, so nothing but
// this catches a TLD that was mistyped, retired, or never existed. Left
// unchecked it would sit in the repo reading exactly like data.

const taxonomy = JSON.parse(await readFile(resolve(here, '../tld-categories.json'), 'utf8'));
const facts = JSON.parse(await readFile(resolve(here, '../tld-facts.json'), 'utf8'));
const index = topicIndex(taxonomy);

check('every category has a name, a description and TLDs',
  Object.entries(taxonomy.categories).filter(([, c]) => !c.name || !c.desc || !c.tlds?.length).map(([id]) => id), []);

check('no TLD is listed twice inside one category',
  Object.entries(taxonomy.categories)
    .filter(([, c]) => new Set(c.tlds).size !== c.tlds.length)
    .map(([id]) => id), []);

check('every TLD named is one IANA lists',
  [...index.keys()].filter((t) => !(t in facts.tlds)), []);

check('none of them is a retired delegation',
  [...index.keys()].filter((t) => facts.tlds[t] && !facts.tlds[t].in_root_zone), []);

// .apple is not a fruit TLD. A brand's subject is its owner, which the derived
// `brand` category already records.
check('no brand TLD has been given a subject',
  [...index.keys()].filter((t) => facts.tlds[t]?.categories.includes('brand')), []);

check('punycode, not unicode, like everywhere else in this repo',
  [...index.keys()].filter((t) => !/^[a-z0-9.-]+$/.test(t)), []);

// The inversion is what the harvester actually consumes.
check('a TLD in two categories keeps both', index.get('kitchen'), ['food', 'home']);
check('a TLD in one keeps one', index.get('pizza'), ['food']);
check('a TLD in none is absent', index.has('xyz'), false);

// --- report ----------------------------------------------------------------

if (failures.length) {
  for (const f of failures) console.log(`FAIL  ${f}`);
  console.log(`\n${failures.length}/${checks} failed`);
  process.exit(1);
}
console.log(`ok — ${checks} checks: the parsers against scripts/fixtures/, the taxonomy against tld-facts.json`);
