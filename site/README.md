# Documentation site

The site at <https://telvax.github.io/telvax/>, built with [Starlight](https://starlight.astro.build) (Astro).

The pages are not written here. `npm run sync` copies `docs/guide/*.md`, `docs/reference/std.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`,
`CODE_OF_CONDUCT.md` and the two licenses into `src/content/docs/`, adds the title Starlight needs and rewrites the links (to other pages, or to the
repository on GitHub). Edit the files under `docs/`; the copies are ignored by git. Only `src/content/docs/index.mdx`,
the home page, is written here.

Code blocks marked `telvax` (or `tvx`) are highlighted with `editors/vscode/syntaxes/telvax.tmLanguage.json`,
the grammar of the VS Code extension.

```sh
cd site
npm ci
npm run dev        # http://localhost:4321/telvax/
npm test           # the link rewriting
npm run build && node scripts/check-links.mjs   # what CI runs
```

`.github/workflows/site.yml` builds the site and checks its links on every pull request that touches the
documentation, and publishes it to GitHub Pages when the change reaches `main`. `SITE_URL` and `SITE_BASE` (for
example `https://telvax.dev` and `/`) set the address if the site moves to its own domain.

To add a page to the site, add it to `PAGES` in `scripts/convert.mjs`.
