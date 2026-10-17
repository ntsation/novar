// Checks the built site: every internal link points to a page that exists, and every `#anchor` to an id
// in that page. Run after `npm run build`.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const dist = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'dist');
const base = (process.env.SITE_BASE ?? '/telvax').replace(/\/$/, '');

function* htmlFiles(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) yield* htmlFiles(p);
    else if (e.name.endsWith('.html')) yield p;
  }
}

const pages = new Map(); // site path ("/guide/tour/") -> ids in the page
for (const file of htmlFiles(dist)) {
  const rel = '/' + path.relative(dist, file).split(path.sep).join('/');
  const url = rel.endsWith('/index.html') ? rel.slice(0, -'index.html'.length) : rel;
  const html = fs.readFileSync(file, 'utf8');
  pages.set(url, { html, ids: new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])) });
}

const problems = [];
for (const [url, { html }] of pages) {
  for (const [, href] of html.matchAll(/<a\s[^>]*?href="([^"]*)"/g)) {
    if (/^([a-z][a-z0-9+.-]*:|\/\/)/i.test(href)) continue; // external, mailto:
    const target = new URL(href.replace(/&amp;/g, '&'), `http://site${base}${url}`);
    const [pathname, hash] = [decodeURIComponent(target.pathname), target.hash.slice(1)];
    if (!pathname.startsWith(base + '/') && pathname !== base) {
      problems.push(`${url}: ${href} leaves the site`);
      continue;
    }
    let site = pathname.slice(base.length) || '/';
    const page = pages.get(site) ?? pages.get(site.replace(/\/?$/, '/')) ?? pages.get(site);
    if (!page && !fs.existsSync(path.join(dist, site))) problems.push(`${url}: ${href} does not exist`);
    else if (page && hash && !page.ids.has(decodeURIComponent(hash))) problems.push(`${url}: ${href} has no anchor #${hash}`);
  }
}

if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
console.log(`${pages.size} pages, links ok`);
