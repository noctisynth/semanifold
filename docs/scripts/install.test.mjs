import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..',
);
const installer = path.join(repositoryRoot, 'docs/public/install/install.sh');

async function fixture(releasePages) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'semifold-install-'));
  const bin = path.join(root, 'bin');
  const installDirectory = path.join(root, 'install');
  const releasePagesDirectory = path.join(root, 'release-pages');
  const curlLog = path.join(root, 'curl.log');
  await mkdir(bin);
  await mkdir(releasePagesDirectory);
  await Promise.all(
    releasePages.map((content, index) =>
      writeFile(
        path.join(releasePagesDirectory, `page-${index + 1}.html`),
        content,
      ),
    ),
  );
  await writeFile(
    path.join(bin, 'uname'),
    `#!/bin/sh
case "$1" in
  -s) printf '%s\\n' Linux ;;
  -m) printf '%s\\n' x86_64 ;;
  *) exit 1 ;;
esac
`,
  );
  await writeFile(
    path.join(bin, 'curl'),
    `#!/bin/sh
output=''
url=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output="$2"
      shift 2
      ;;
    -H)
      shift 2
      ;;
    -*)
      shift
      ;;
    *)
      url="$1"
      shift
      ;;
  esac
done
printf '%s\\n' "$url" >> "$SEMIFOLD_CURL_LOG"
case "$url" in
  https://github.com/noctisynth/semifold/releases?page=*)
    page="\${url##*=}"
    fixture="$SEMIFOLD_RELEASE_PAGES/page-$page.html"
    if [ -f "$fixture" ]; then
      cat "$fixture"
    else
      printf '%s\\n' '<html></html>'
    fi
    ;;
  https://github.com/*)
    printf '%s' semifold-binary > "$output"
    ;;
  *)
    exit 1
    ;;
esac
`,
  );
  await chmod(path.join(bin, 'uname'), 0o755);
  await chmod(path.join(bin, 'curl'), 0o755);

  return {
    cleanup: () => rm(root, { force: true, recursive: true }),
    curlLog,
    env: {
      ...process.env,
      HOME: root,
      PATH: `${bin}${path.delimiter}${process.env.PATH}`,
      SEMIFOLD_CURL_LOG: curlLog,
      SEMIFOLD_RELEASE_PAGES: releasePagesDirectory,
    },
    installDirectory,
  };
}

function runInstaller(current, ...args) {
  return spawnSync(
    'sh',
    [installer, ...args, '--install-dir', current.installDirectory],
    { encoding: 'utf8', env: current.env },
  );
}

test('resolves the latest stable Semifold binary release dynamically', async () => {
  const current = await fixture([
    `<a href="/noctisynth/semifold/releases/tag/%40semifold%2Fcli-v9.9.9">CLI</a>
<a href="/noctisynth/semifold/releases/tag/semifold-v9.0.0-rc.1">Prerelease</a>
<a rel="next" href="/noctisynth/semifold/releases?page=2">Next</a>`,
    `<a href="/noctisynth/semifold/releases/tag/semifold-v0.3.1">Latest binary</a>
<a href="/noctisynth/semifold/releases/tag/semifold-v0.3.0">Older binary</a>`,
  ]);
  try {
    const result = runInstaller(current);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /version 0\.3\.1/);
    assert.equal(
      await readFile(path.join(current.installDirectory, 'semifold'), 'utf8'),
      'semifold-binary',
    );
    const requests = await readFile(current.curlLog, 'utf8');
    assert.match(requests, /releases\?page=1/);
    assert.match(requests, /releases\?page=2/);
    assert.match(
      requests,
      /releases\/download\/semifold-v0\.3\.1\/semifold-linux-x86_64/,
    );
    assert.doesNotMatch(requests, /releases\/latest/);
  } finally {
    await current.cleanup();
  }
});

for (const [version, normalized] of [
  ['0.3.1', '0.3.1'],
  ['v0.3.1', '0.3.1'],
  ['0.4.0-rc.1', '0.4.0-rc.1'],
]) {
  test(`normalizes explicit version ${version}`, async () => {
    const current = await fixture([]);
    try {
      const result = runInstaller(current, version);
      assert.equal(result.status, 0, result.stderr);
      const requests = await readFile(current.curlLog, 'utf8');
      assert.equal(
        requests.trim(),
        `https://github.com/noctisynth/semifold/releases/download/semifold-v${normalized}/semifold-linux-x86_64`,
      );
    } finally {
      await current.cleanup();
    }
  });
}

test('fails without a stable binary release and does not install a file', async () => {
  const current = await fixture([
    `<a href="/noctisynth/semifold/releases/tag/%40semifold%2Fcli-v0.3.1">CLI</a>
<a href="/noctisynth/semifold/releases/tag/semifold-v0.4.0-rc.1">Prerelease</a>`,
  ]);
  try {
    const result = runInstaller(current);
    assert.notEqual(result.status, 0);
    assert.match(
      result.stderr,
      /Failed to resolve the latest stable Semifold release/,
    );
    await assert.rejects(
      readFile(path.join(current.installDirectory, 'semifold')),
      { code: 'ENOENT' },
    );
  } finally {
    await current.cleanup();
  }
});
