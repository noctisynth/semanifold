import { readdir } from 'node:fs/promises';
import { relative, resolve } from 'node:path';

const contentRoot = resolve('content/docs');

async function collectFiles(root, directory = root) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const path = resolve(directory, entry.name);
      if (entry.isDirectory()) return collectFiles(root, path);
      if (!entry.name.endsWith('.mdx') && entry.name !== 'meta.json') return [];
      return [relative(root, path)];
    }),
  );

  return files.flat().sort();
}

const english = await collectFiles(resolve(contentRoot, 'en'));
const chinese = await collectFiles(resolve(contentRoot, 'zh'));

if (english.join('\n') !== chinese.join('\n')) {
  const englishOnly = english.filter((path) => !chinese.includes(path));
  const chineseOnly = chinese.filter((path) => !english.includes(path));
  throw new Error(
    [
      'English and Chinese documentation paths are not in parity.',
      `English only: ${englishOnly.join(', ') || 'none'}`,
      `Chinese only: ${chineseOnly.join(', ') || 'none'}`,
    ].join('\n'),
  );
}

console.log(`Locale parity: ${english.length} files per language.`);
