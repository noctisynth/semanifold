import {
  createPluginDiagnostic,
  createPluginFailure,
  createPluginSuccess,
  definePlugin,
  definePluginMetadata,
  type PluginDiscoverRequestV1,
  type PluginEditSourceV1,
  type PluginFetchResponseV1,
  type PluginPackageInspectionV1,
  type PluginUrlV1,
} from '@semifold/plugin-sdk';

export const metadata = definePluginMetadata({
  ecosystem: 'com.example.engine',
  pluginVersion: '1.0.0',
  readPatterns: ['packages/**/engine.json'],
});

const workspaceManifestEdit: PluginEditSourceV1 = {
  kind: 'workspace-manifest',
  'shared-versions': [],
  dependencies: [],
};

void workspaceManifestEdit;

const metadataSchemaVersion: 1 = metadata['schema-version'];
void metadataSchemaVersion;

// @ts-expect-error Generated wire arrays remain deeply readonly at the public boundary.
metadata.operations.push('discover');

const inspection: PluginPackageInspectionV1 = {
  id: 'engine',
  'manifest-name': 'engine',
  version: '1.0.0',
  'version-source': { kind: 'package-manifest' },
  ecosystem: metadata.ecosystem,
  path: 'packages/engine',
  publishable: true,
  dependencies: [
    {
      'manifest-name': 'runtime',
      kind: 'runtime',
      requirement: '^1.0.0',
    },
  ],
};

export default definePlugin(async (request, host) => {
  switch (request.operation) {
    case 'discover': {
      const paths = await host.listFiles('packages/**/engine.json');
      await Promise.all(paths.map((path) => host.readText(path)));

      const endpoint = new URL('/packages', 'https://api.example.test');
      const response = await fetch(endpoint.toString(), {
        headers: { accept: 'application/json' },
        method: 'GET',
      });
      response.headers.get('content-type');
      response.headers.entries();
      response.bytes();
      response.json();
      response.text();

      if (response.status >= 400) {
        return createPluginFailure(request, metadata.ecosystem, {
          code: 'remote-discovery-failed',
          message: response.statusText,
        });
      }

      const warning = createPluginDiagnostic(request, metadata.ecosystem, {
        severity: 'warning',
        code: 'remote-discovery',
        message: `Loaded ${paths.length} manifests from ${endpoint.origin}`,
      });
      return createPluginSuccess(request, { packages: [inspection] }, [
        warning,
      ]);
    }
    case 'inspect':
      return createPluginSuccess(request, {
        package: {
          ...inspection,
          id: request.input.package.id,
          path: request.input.package.path,
        },
      });
    case 'plan-edits':
      return createPluginSuccess(request, {
        edits: [
          {
            path: `${inspection.path}/engine.json`,
            expected: {
              kind: 'existing',
              sha256:
                '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
            },
            'new-content': '{}\n',
            source: {
              kind: 'package-version',
              package: inspection.id,
            },
          },
        ],
      });
  }
});

function assertUnavailableAmbientTypes(
  response: PluginFetchResponseV1,
  endpoint: PluginUrlV1,
): void {
  // @ts-expect-error Boa's response subset does not expose browser Response.ok.
  void response.ok;
  // @ts-expect-error Boa currently does not implement URL.searchParams.
  void endpoint.searchParams;
  // @ts-expect-error The SDK must not expose Node.js process globals.
  void process;
  // @ts-expect-error The SDK must not expose Node.js Buffer globals.
  void Buffer;
  // @ts-expect-error The SDK must not expose browser window globals.
  void window;
  // @ts-expect-error The SDK must not expose browser document globals.
  void document;
}

void assertUnavailableAmbientTypes;

function assertOperationCorrelation(request: PluginDiscoverRequestV1): void {
  const requestSchemaVersion: 1 = request['schema-version'];
  void requestSchemaVersion;

  // @ts-expect-error Generated nested request fields remain readonly.
  request.input['project-root'] = '..';

  // @ts-expect-error A discover request cannot be paired with plan-edits output.
  createPluginSuccess(request, { edits: [] });
}

void assertOperationCorrelation;
