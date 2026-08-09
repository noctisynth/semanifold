import { access, readdir, readFile } from 'node:fs/promises';
import { extname, resolve } from 'node:path';

const outputRoot = resolve('out');
const requiredFiles = [
  'index.html',
  '404.html',
  'docs/index.html',
  'docs/concepts/glossary/index.html',
  'docs/configuration/reference/index.html',
  'docs/getting-started/first-release/index.html',
  'docs/plugins/overview/index.html',
  'guide/start/quick-start/index.html',
  'en/guide/start/quick-start/index.html',
  'zh/index.html',
  'zh/docs/index.html',
  'zh/docs/concepts/glossary/index.html',
  'zh/docs/configuration/reference/index.html',
  'zh/docs/getting-started/first-release/index.html',
  'zh/docs/plugins/overview/index.html',
  'zh/guide/start/quick-start/index.html',
  'api/search',
  'llms.txt',
  'llms-full.txt',
];

await Promise.all(
  requiredFiles.map((path) => access(resolve(outputRoot, path))),
);

const search = JSON.parse(
  await readFile(resolve(outputRoot, 'api/search'), 'utf8'),
);
if (search.i18n !== true) {
  throw new Error(
    'The static search index is not configured for locale filtering.',
  );
}

async function collectHtml(directory = outputRoot) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const path = resolve(directory, entry.name);
      if (entry.isDirectory()) return collectHtml(path);
      return entry.name.endsWith('.html') ? [path] : [];
    }),
  );
  return files.flat();
}

async function targetExists(pathname) {
  const decoded = decodeURIComponent(pathname);
  const relativePath = decoded.replace(/^\/+/, '').replace(/\/$/, '');
  const candidates = relativePath
    ? extname(relativePath)
      ? [relativePath]
      : [relativePath, `${relativePath}.html`, `${relativePath}/index.html`]
    : ['index.html'];

  for (const candidate of candidates) {
    try {
      await access(resolve(outputRoot, candidate));
      return true;
    } catch {
      // Try the next static-export shape.
    }
  }
  return false;
}

const broken = [];
for (const htmlPath of await collectHtml()) {
  const html = await readFile(htmlPath, 'utf8');
  const links = html.matchAll(/href="(\/[^"]*)"/g);
  for (const match of links) {
    const pathname = match[1]?.split(/[?#]/, 1)[0];
    if (!pathname || (await targetExists(pathname))) continue;
    broken.push(`${htmlPath}: ${pathname}`);
  }
}

if (broken.length > 0) {
  throw new Error(
    `Broken internal links in static output:\n${broken.join('\n')}`,
  );
}

console.log(
  'Static output: required routes, locale search, and internal links passed.',
);
