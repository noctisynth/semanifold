import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  createPluginDiagnostic,
  createPluginFailure,
  createPluginSuccess,
  definePluginMetadata,
} from '../dist/index.js';

const discoverRequest = {
  'schema-version': 1,
  operation: 'discover',
  input: { 'project-root': '.' },
};

function fixture(name) {
  return JSON.parse(
    readFileSync(new URL(`./fixtures/${name}`, import.meta.url), 'utf8'),
  );
}

test('metadata is canonical, immutable, and omits empty read patterns', () => {
  const metadata = definePluginMetadata({
    ecosystem: 'com.example.engine',
    pluginVersion: '1.2.3',
    readPatterns: ['zeta.json', 'alpha.json', 'zeta.json'],
  });

  assert.deepEqual(metadata, fixture('plugin-metadata-v1.json'));
  assert.equal(Object.isFrozen(metadata), true);
  assert.equal(Object.isFrozen(metadata.operations), true);
  assert.equal(Object.isFrozen(metadata['read-patterns']), true);

  assert.deepEqual(
    definePluginMetadata({
      ecosystem: 'com.example.engine',
      pluginVersion: '1.2.3',
    }),
    {
      'schema-version': 1,
      ecosystem: 'com.example.engine',
      'plugin-version': '1.2.3',
      operations: ['discover', 'inspect', 'plan-edits'],
    },
  );
});

test('success responses preserve the Rust schema v1 wire shape', () => {
  const diagnostic = createPluginDiagnostic(
    discoverRequest,
    'com.example.engine',
    {
      severity: 'info',
      code: 'discovery-started',
      message: 'Discovery started',
      path: 'packages',
    },
  );
  const response = createPluginSuccess(discoverRequest, { packages: [] }, [
    diagnostic,
  ]);

  assert.deepEqual(response, fixture('plugin-discover-success-v1.json'));
});

test('failure responses always contain a correlated error diagnostic', () => {
  const warning = createPluginDiagnostic(
    discoverRequest,
    'com.example.engine',
    {
      severity: 'warning',
      code: 'partial-discovery',
      message: 'Some manifests were ignored',
    },
  );
  const response = createPluginFailure(
    discoverRequest,
    'com.example.engine',
    {
      code: 'discovery-failed',
      message: 'Discovery failed',
      package: 'engine',
    },
    [warning],
  );

  assert.deepEqual(response, fixture('plugin-discover-failure-v1.json'));
});
