// Turns a Markdown file of the repository into a page of the site.
//
// The pages stay where they are (`docs/guide`, `docs/design/status.md`) so that a change and its
// documentation are one pull request. This script only adds what Starlight needs: a front matter with the
// title, and links that work on the site.

import path from 'node:path';

const REPO = 'https://github.com/telvax/telvax';

/** Source file (from the repository root) -> where it is on the site. */
export const PAGES = [
  { src: 'docs/guide/tour.md', out: 'guide/tour', order: 1 },
  { src: 'docs/guide/use-cases.md', out: 'guide/use-cases', order: 2 },
  { src: 'docs/guide/stability.md', out: 'guide/stability', order: 3 },
  { src: 'docs/design/status.md', out: 'status', order: 1 },
];

/** Rewrites one link target found in `from` (a path in the repository). */
export function rewriteLink(target, from, pages = PAGES) {
  if (/^([a-z][a-z0-9+.-]*:|#|\/\/)/i.test(target)) return target;
  const [file, hash = ''] = target.split('#');
  const anchor = hash ? `#${hash}` : '';
  const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(from), file));
  const page = pages.find((p) => p.src === resolved);
  if (page) {
    const here = pages.find((p) => p.src === from);
    const rel = path.posix.relative(here.out, page.out) || '.';
    return `${rel}/${anchor}`;
  }
  // Anything else is in the repository: files open in the browser, folders list their content.
  const kind = path.posix.extname(resolved) ? 'blob' : 'tree';
  return `${REPO}/${kind}/main/${resolved}${anchor}`;
}

/** Applies `fn` to the text outside fenced code blocks. */
function outsideFences(text, fn) {
  let fence = null;
  return text
    .split('\n')
    .map((line) => {
      const m = line.match(/^\s*(`{3,}|~{3,})/);
      if (m) {
        if (!fence) fence = m[1][0];
        else if (m[1][0] === fence) fence = null;
        return line;
      }
      return fence ? line : fn(line);
    })
    .join('\n');
}

/** The site page for `page.src`, given the file's text. */
export function convert(page, text, pages = PAGES) {
  const lines = text.replace(/\r\n/g, '\n').split('\n');
  const h1 = lines.findIndex((l) => /^# /.test(l));
  if (h1 === -1) throw new Error(`${page.src}: no "# Title" line`);
  const title = lines[h1].slice(2).trim();
  lines.splice(h1, 1);
  // Links like `](other.md)`; a target with a space or a comma is code such as `](self, f)`.
  const body = outsideFences(lines.join('\n'), (line) =>
    line.replace(/\]\(([^)\s,]+)\)/g, (_, t) => `](${rewriteLink(t, page.src, pages)})`),
  ).replace(/^\n+/, '');
  const front = [
    '---',
    `title: ${JSON.stringify(title)}`,
    `editUrl: ${REPO}/edit/main/${page.src}`,
    `sidebar:\n  order: ${page.order}`,
    '---',
    '',
  ].join('\n');
  return front + body;
}
