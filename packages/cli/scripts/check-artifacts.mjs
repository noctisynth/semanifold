import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

async function visibleEntries(directory) {
  return (await readdir(directory, { withFileTypes: true }))
    .filter(({ name }) => !name.startsWith('.'))
    .sort((left, right) => left.name.localeCompare(right.name));
}

export async function checkArtifacts({ artifactsDirectory, packageRoot }) {
  const npmDirectory = path.join(packageRoot, 'npm');
  const rootManifest = JSON.parse(
    await readFile(path.join(packageRoot, 'package.json'), 'utf8'),
  );
  const packageDirectories = (await visibleEntries(npmDirectory)).filter(
    (entry) => entry.isDirectory(),
  );
  assert.equal(packageDirectories.length, rootManifest.napi.targets.length);

  const expectedArtifacts = [];
  for (const directory of packageDirectories) {
    const platformRoot = path.join(npmDirectory, directory.name);
    const manifest = JSON.parse(
      await readFile(path.join(platformRoot, 'package.json'), 'utf8'),
    );
    assert.equal(manifest.name, `${rootManifest.name}-${directory.name}`);
    assert.equal(manifest.version, rootManifest.version);
    assert.match(manifest.main, /^semifold\..+\.node$/);
    assert.equal(manifest.os.length, 1);
    assert.equal(manifest.cpu.length, 1);
    if (directory.name.startsWith('linux-')) {
      assert.deepEqual(manifest.libc, ['glibc']);
    }

    const nativeFiles = (await visibleEntries(platformRoot))
      .filter(({ name }) => name.endsWith('.node'))
      .map(({ name }) => name);
    assert.deepEqual(nativeFiles, [manifest.main]);
    expectedArtifacts.push(manifest.main);
  }

  const actualArtifacts = (await visibleEntries(artifactsDirectory)).map(
    ({ name }) => name,
  );
  assert.deepEqual(actualArtifacts, expectedArtifacts.sort());
}

const isMain = process.argv[1]
  ? import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
  : false;
if (isMain) {
  const packageRoot = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    '..',
  );
  await checkArtifacts({
    artifactsDirectory: path.resolve(
      process.argv[2] ?? path.join(packageRoot, 'artifacts'),
    ),
    packageRoot,
  });
}
