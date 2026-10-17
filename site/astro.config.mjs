import fs from 'node:fs';
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// Code blocks tagged `telvax` (or `tvx`) use the same TextMate grammar as the VS Code extension.
const grammar = JSON.parse(
  fs.readFileSync(new URL('../editors/vscode/syntaxes/telvax.tmLanguage.json', import.meta.url), 'utf8'),
);

export default defineConfig({
  // GitHub Pages serves a repository at https://<org>.github.io/<repo>/. A custom domain sets
  // SITE_URL and SITE_BASE=/ in the workflow.
  site: process.env.SITE_URL ?? 'https://telvax.github.io',
  base: process.env.SITE_BASE ?? '/telvax',
  integrations: [
    starlight({
      title: 'Telvax',
      description: 'A statically typed, compiled language for data, ML pipelines and AI serving.',
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/telvax/telvax' }],
      editLink: { baseUrl: 'https://github.com/telvax/telvax/edit/main/' },
      expressiveCode: {
        shiki: {
          langs: [{ ...grammar, name: 'telvax', aliases: ['tvx'] }],
        },
      },
      sidebar: [
        { label: 'Guide', items: [{ autogenerate: { directory: 'guide' } }] },
        { label: 'Reference', items: [{ autogenerate: { directory: 'reference' } }] },
        { label: 'Project status', link: '/status/' },
      ],
    }),
  ],
});
