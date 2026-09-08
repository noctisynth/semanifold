import { access, readdir, readFile } from 'node:fs/promises';
import { extname, resolve } from 'node:path';

const outputRoot = resolve('out');
const requiredFiles = [
  'index.html',
  '404.html',
  'docs/index.html',
  'docs/concepts/glossary/index.html',
  'docs/commands/ci/index.html',
  'docs/commands/reference/index.html',
  'docs/configuration/reference/index.html',
  'docs/getting-started/first-release/index.html',
  'docs/plugins/overview/index.html',
  'guide/start/quick-start/index.html',
  'en/guide/start/quick-start/index.html',
  'zh/index.html',
  'zh/docs/index.html',
  'zh/docs/concepts/glossary/index.html',
  'zh/docs/commands/ci/index.html',
  'zh/docs/commands/reference/index.html',
  'zh/docs/configuration/reference/index.html',
  'zh/docs/getting-started/first-release/index.html',
  'zh/docs/plugins/overview/index.html',
  'zh/guide/start/quick-start/index.html',
  'api/search',
  'llms.txt',
  'zh/llms.txt',
  'markdown/en/index.md',
  'markdown/zh/index.md',
  'markdown/en/agents.md',
  'markdown/zh/agents.md',
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

// Validate the actual exported Markdown linked from both agent indexes.
for (const [locale, index] of [['en', 'llms.txt'], ['zh', 'zh/llms.txt']]) {
  const content = await readFile(resolve(outputRoot, index), 'utf8');
  if ((content.match(/^# /gm) ?? []).length !== 1) {
    throw new Error(`${index} must contain exactly one document title.`);
  }
  const links = [...content.matchAll(/\]\((https:\/\/semifold\.noctisynth\.org\/markdown\/[^)]+)\)/g)];
  if (links.length === 0) throw new Error(`${index} contains no Markdown links.`);
  for (const [, link] of links) {
    const path = new URL(link).pathname;
    if (!path.startsWith(`/markdown/${locale}/`)) throw new Error(`Mixed locale in ${index}: ${path}`);
    const markdown = await readFile(resolve(outputRoot, path.slice(1)), 'utf8');
    if (!markdown.includes(`Language: ${locale}`)) throw new Error(`Missing locale: ${path}`);
    const source = markdown.match(/^Source: (https:\/\/[^\s]+)/m)?.[1];
    if (!source) throw new Error(`Missing source URL: ${path}`);
    const html = await readFile(resolve(outputRoot, new URL(source).pathname.slice(1), 'index.html'), 'utf8');
    const tags = [...html.matchAll(/<link\b[^>]*>/g)].map((match) => match[0]);
    if (!tags.some((tag) => tag.includes('rel="alternate"') && tag.includes('type="text/markdown"') &&
      (tag.includes(`href="${path}"`) || tag.includes(`href="https://semifold.noctisynth.org${path}"`)))) {
      throw new Error(`Missing HTML Markdown alternate: ${source}`);
    }
  }
}
console.log('Agent output: locale indexes, Markdown bodies, and HTML discovery links passed.');
