// Turns the repo's docs/ directory into the site's docs collection.
//
// Those files are canonical — they ship inside every .tar.gz, .deb and .dmg —
// so the site generates its pages from them rather than keeping a second copy
// that would drift within a release. Nothing under src/content/docs or
// src/content/pages is hand-edited.

import { readFile, writeFile, mkdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
const docsDir = resolve(repo, 'docs');
const outDir = resolve(here, '../src/content/docs');
const REPO_URL = 'https://github.com/AminulBD/ds';

// The sidebar, in order. A file not named here is not published — which is how
// docs/README.md (an index the site does not need) stays out.
const SIDEBAR = [
  'usage',
  'choosing-tlds',
  'showing-more',
  'prices',
  'the-az-of-tlds',
  'choosing-the-source',
  'pacing',
  'accuracy-notes',
  'the-bundled-whois-table',
  'http-api',
];

// Pages of their own rather than a docs sidebar slot.
const STANDALONE = { install: '/install' };

/** Where a docs/<slug>.md file lives on the site. */
const routeOf = (slug) => STANDALONE[slug] ?? `/docs/${slug}`;

/** Rewrite repo-relative links so they resolve on the site. */
function rewrite(body) {
  return (
    body
      // docs/<slug>.md[#anchor] -> the route that page now lives at
      .replace(/\]\(([\w-]+)\.md(#[\w-]+)?\)/g, (m, slug, anchor) =>
        SIDEBAR.includes(slug) || slug in STANDALONE
          ? `](${routeOf(slug)}${anchor ?? ''})`
          : m,
      )
      // ../<repo file> -> a GitHub blob link
      .replace(
        /\]\(\.\.\/(LICENSE|README\.md|[\w.-]+\.json|ds\.1|scripts\/[\w.-]+)\)/g,
        `](${REPO_URL}/blob/main/$1)`,
      )
  );
}

/** Read one docs file, returning its `# Title` and the body beneath it. */
async function load(slug) {
  const md = await readFile(resolve(docsDir, `${slug}.md`), 'utf8');
  const lines = md.split('\n');
  const i = lines.findIndex((l) => l.startsWith('# '));
  if (i === -1) throw new Error(`docs/${slug}.md has no '# Title' heading`);
  // The layout prints the title itself, so the h1 does not go into the body.
  return { slug, title: lines[i].slice(2).trim(), body: lines.slice(i + 1).join('\n') };
}

await rm(outDir, { recursive: true, force: true });
await mkdir(outDir, { recursive: true });

let order = 0;
for (const slug of SIDEBAR) {
  const page = await load(slug);
  order += 1;
  const fm = ['---', `title: ${JSON.stringify(page.title)}`, `order: ${order}`, '---', ''].join('\n');
  await writeFile(resolve(outDir, `${slug}.md`), `${fm}${rewrite(page.body).trim()}\n`);
}

// Install gets a page of its own, laid out by src/pages/install.astro, so it
// goes to a separate collection.
const pagesDir = resolve(here, '../src/content/pages');
await rm(pagesDir, { recursive: true, force: true });
await mkdir(pagesDir, { recursive: true });
for (const slug of Object.keys(STANDALONE)) {
  const page = await load(slug);
  await writeFile(
    resolve(pagesDir, `${slug}.md`),
    `---\ntitle: ${JSON.stringify(page.title)}\n---\n\n${rewrite(page.body).trim()}\n`,
  );
}

console.log(`  docs     ${SIDEBAR.length} pages — ${SIDEBAR.join(', ')}`);
