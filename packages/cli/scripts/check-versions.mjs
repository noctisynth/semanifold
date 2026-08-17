import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
);
const rootManifest = JSON.parse(
  await readFile(path.join(packageRoot, 'package.json'), 'utf8'),
);
const cargoManifest = await readFile(
  path.join(packageRoot, '../../crates/semifold/Cargo.toml'),
  'utf8',
);
const rustVersion = cargoManifest.match(/^version = "([^"]+)"$/m)?.[1];

assert.equal(rootManifest.version, rustVersion);
assert.equal(rootManifest.napi.binaryName, 'semifold');
assert.deepEqual([...rootManifest.napi.targets].sort(), [
  'aarch64-apple-darwin',
  'aarch64-pc-windows-msvc',
  'aarch64-unknown-linux-gnu',
  'x86_64-apple-darwin',
  'x86_64-pc-windows-msvc',
  'x86_64-unknown-linux-gnu',
]);
