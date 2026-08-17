import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
);
const packageManifest = JSON.parse(
  readFileSync(path.join(packageRoot, 'package.json'), 'utf8'),
);
const npmCacheDirectory = mkdtempSync(
  path.join(os.tmpdir(), 'semifold-npm-cache-'),
);

function packedFiles(directory) {
  const output = execFileSync(
    'npm',
    ['pack', '--dry-run', '--json', '--ignore-scripts'],
    {
      cwd: directory,
      encoding: 'utf8',
      env: { ...process.env, npm_config_cache: npmCacheDirectory },
    },
  );
  const result = JSON.parse(output);
  assert.equal(result.length, 1);
  return result[0].files.map(({ path: file }) => file).sort();
}

try {
  const wrapperFiles = packedFiles(packageRoot);
  for (const required of ['bin/semifold.js', 'index.js', 'index.d.ts']) {
    assert(wrapperFiles.includes(required), required);
  }

  const npmDirectory = path.join(packageRoot, 'npm');
  const platformDirectories = readdirSync(npmDirectory, {
    withFileTypes: true,
  })
    .filter((entry) => entry.isDirectory())
    .sort((left, right) => left.name.localeCompare(right.name));
  assert.equal(platformDirectories.length, packageManifest.napi.targets.length);

  for (const entry of platformDirectories) {
    const directory = path.join(npmDirectory, entry.name);
    const platformManifest = JSON.parse(
      readFileSync(path.join(directory, 'package.json'), 'utf8'),
    );
    const files = packedFiles(directory);
    assert(files.includes(platformManifest.main), platformManifest.main);
    assert.equal(files.filter((file) => file.endsWith('.node')).length, 1);
  }
} finally {
  rmSync(npmCacheDirectory, { force: true, recursive: true });
}
