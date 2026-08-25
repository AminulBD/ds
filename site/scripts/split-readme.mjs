// Splits the repo README into one page per `## ` section.
//
// The README is canonical — it ships inside every .tar.gz, .deb and .dmg — so
// the site generates its docs from it rather than keeping a second copy that
// would drift within a release. Nothing under src/content/docs is hand-edited.

import { readFile, writeFile, mkdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
const outDir = resolve(here, '../src/content/docs');
const REPO_URL = 'https://github.com/AminulBD/ds';

/** GitHub's heading-anchor rules, near enough for the headings this README has. */
const slugify = (s) =>
  s.toLowerCase().trim()
    .replace(/[^\w\s-]/g, '')
    .replace(/\s+/g, '-');

// Sections lifted out of the docs sidebar onto pages of their own.
const STANDALONE = { Install: '/install' };
// Sections that belong on the site but not in the sidebar: they describe the
// repo, not the tool, and the GitHub README is the right home for them.
const SKIP = new Set(['Development', 'Releases', 'Licence']);

const md = await readFile(resolve(repo, 'README.md'), 'utf8');
const lines = md.split('\n');

// --- carve into sections on `## `, tracking fenced blocks so a `##` inside a
// --- code fence never starts one ---
const sections = [];
let intro = [];
let cur = null;
let fenced = false;
for (const line of lines) {
  if (line.startsWith('```')) fenced = !fenced;
  const h2 = !fenced && line.match(/^## (.+)$/);
  if (h2) {
    if (cur) sections.push(cur);
    cur = { title: h2[1].trim(), body: [] };
  } else if (cur) {
    cur.body.push(line);
  } else {
    intro.push(line);
  }
}
if (cur) sections.push(cur);

// --- anchor map: every h2/h3 anchor -> the route it now lives at ---
const anchors = new Map();
for (const s of sections) {
  const slug = slugify(s.title);
  const route = STANDALONE[s.title] ?? (SKIP.has(s.title) ? `${REPO_URL}#${slug}` : `/docs/${slug}`);
  s.slug = slug;
  s.route = route;
  anchors.set(slug, route);
  let f = false;
  for (const line of s.body) {
    if (line.startsWith('```')) f = !f;
    const h3 = !f && line.match(/^### (.+)$/);
    if (h3) anchors.set(slugify(h3[1]), `${route}#${slugify(h3[1])}`);
  }
}

/** Rewrite README-relative links so they resolve on the site. */
function rewrite(body) {
  return body
    // in-README anchors -> the page that heading now lives on
    .replace(/\]\(#([\w-]+)\)/g, (m, a) => (anchors.has(a) ? `](${anchors.get(a)})` : m))
    // ../../releases -> the real releases page
    .replace(/\]\(\.\.\/\.\.\/releases\)/g, `](${REPO_URL}/releases)`)
    // repo files -> GitHub blob links
    .replace(
      /\]\((LICENSE|whois\.json|pricing\.json|ds\.1|scripts\/[\w.-]+)\)/g,
      `](${REPO_URL}/blob/main/$1)`,
    );
}

await rm(outDir, { recursive: true, force: true });
await mkdir(outDir, { recursive: true });

let order = 0;
const written = [];
for (const s of sections) {
  if (SKIP.has(s.title) || STANDALONE[s.title]) continue;
  order += 1;
  const body = rewrite(s.body.join('\n')).trim();
  const fm = [
    '---',
    `title: ${JSON.stringify(s.title)}`,
    `order: ${order}`,
    '---',
    '',
  ].join('\n');
  await writeFile(resolve(outDir, `${s.slug}.md`), `${fm}${body}\n`);
  written.push(s.slug);
}

// The Install section gets a page of its own rather than a sidebar slot, so it
// goes to a separate collection; the intro is emitted as data for the landing
// page, which lays it out itself.
const install = sections.find((s) => s.title === 'Install');
const pagesDir = resolve(here, '../src/content/pages');
await rm(pagesDir, { recursive: true, force: true });
await mkdir(pagesDir, { recursive: true });
if (install) {
  await writeFile(
    resolve(pagesDir, 'install.md'),
    `---\ntitle: "Install"\n---\n\n${rewrite(install.body.join('\n')).trim()}\n`,
  );
}
await mkdir(resolve(here, '../src/data'), { recursive: true });
await writeFile(
  resolve(here, '../src/data/readme.json'),
  JSON.stringify({
    intro: rewrite(intro.join('\n').replace(/^# .*$/m, '')).trim(),
    install: install ? rewrite(install.body.join('\n')).trim() : '',
    sections: sections
      .filter((s) => !SKIP.has(s.title) && !STANDALONE[s.title])
      .map((s) => ({ title: s.title, slug: s.slug })),
  }),
);

console.log(`  docs     ${written.length} sections — ${written.join(', ')}`);
