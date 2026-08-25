import { defineCollection, z } from 'astro:content';
import { glob } from 'astro/loaders';

// Both collections are populated by scripts/build-docs.mjs from the repo's
// docs/ directory — never edited by hand.
const docs = defineCollection({
  loader: glob({ pattern: '**/*.md', base: './src/content/docs' }),
  schema: z.object({ title: z.string(), order: z.number() }),
});

// Sections that get a page of their own rather than a docs sidebar slot.
const pages = defineCollection({
  loader: glob({ pattern: '**/*.md', base: './src/content/pages' }),
  schema: z.object({ title: z.string() }),
});

export const collections = { docs, pages };
