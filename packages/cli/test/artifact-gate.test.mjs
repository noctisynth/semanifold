import assert from 'node:assert/strict';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { checkArtifacts } from '../scripts/check-artifacts.mjs';

const platforms = [
  {
    directory: 'darwin-arm64',
    main: 'semifold.darwin-arm64.node',
    manifest: { cpu: ['arm64'], os: ['darwin'] },
  },
  {
    directory: 'linux-x64-gnu',
    main: 'semifold.linux-x64-gnu.node',
    manifest: { cpu: ['x64'], libc: ['glibc'], os: ['linux'] },
  },
];

async function fixture() {
  const packageRoot = await mkdtemp(
    path.join(os.tmpdir(), 'semifold-artifact-gate-'),
  );
  const artifactsDirectory = path.join(packageRoot, 'artifacts');
  await mkdir(artifactsDirectory);
  await writeFile(
    path.join(packageRoot, 'package.json'),
    JSON.stringify({
      name: '@semifold/cli',
      napi: { targets: platforms.map(({ directory }) => directory) },
      version: '0.3.0',
    }),
  );

  for (const platform of platforms) {
    const platformRoot = path.join(packageRoot, 'npm', platform.directory);
    await mkdir(platformRoot, { recursive: true });
    await writeFile(
      path.join(platformRoot, 'package.json'),
      JSON.stringify({
        ...platform.manifest,
        main: platform.main,
        name: `@semifold/cli-${platform.directory}`,
        version: '0.3.0',
      }),
    );
    await writeFile(path.join(platformRoot, platform.main), 'binding');
    await writeFile(path.join(artifactsDirectory, platform.main), 'artifact');
  }

  return { artifactsDirectory, packageRoot };
}

test('accepts a complete napi-rs artifact set', async () => {
  const current = await fixture();
  try {
    await checkArtifacts(current);
  } finally {
    await rm(current.packageRoot, { force: true, recursive: true });
  }
});

test('rejects missing and unknown artifacts before publish', async () => {
  const current = await fixture();
  try {
    await rm(path.join(current.artifactsDirectory, platforms[0].main));
    await writeFile(
      path.join(current.artifactsDirectory, 'semifold.unknown.node'),
      'unknown',
    );
    await assert.rejects(checkArtifacts(current), assert.AssertionError);
  } finally {
    await rm(current.packageRoot, { force: true, recursive: true });
  }
});
