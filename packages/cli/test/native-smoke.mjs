import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
);
const packageManifest = JSON.parse(
  readFileSync(new URL('../package.json', import.meta.url), 'utf8'),
);
const result = spawnSync(
  process.execPath,
  [path.join(packageRoot, 'bin', 'semifold.js'), '--version'],
  {
    cwd: packageRoot,
    encoding: 'utf8',
  },
);

assert.equal(result.status, 0, result.stderr);
assert.equal(result.stdout.trim(), `semifold ${packageManifest.version}`);
