# `@semifold/plugin-sdk`

The schema-versioned wire types are generated from Semifold's Rust plugin protocol. After changing
`crates/resolver/src/plugin/protocol.rs`, regenerate and verify them from the repository root:

```sh
pnpm --filter @semifold/plugin-sdk generate:protocol
pnpm --filter @semifold/plugin-sdk check:protocol
```

The generated source is committed so packaging the SDK never requires Cargo.

TypeScript protocol types and construction helpers for Semifold ecosystem plugins.
The SDK mirrors Semifold's schema v1 wire format and only declares capabilities that
the embedded Boa runtime provides. It does not add browser or Node.js ambient APIs.

```ts
import {
  createPluginSuccess,
  definePlugin,
  definePluginMetadata,
} from '@semifold/plugin-sdk';

export const metadata = definePluginMetadata({
  ecosystem: 'com.example.engine',
  pluginVersion: '1.0.0',
  readPatterns: ['packages/**/engine.json'],
});

export default definePlugin(async (request, host) => {
  switch (request.operation) {
    case 'discover': {
      const paths = await host.listFiles('packages/**/engine.json');
      await Promise.all(paths.map((path) => host.readText(path)));
      return createPluginSuccess(request, { packages: [] });
    }
    case 'inspect':
      throw new Error(`Package ${request.input.package.id} is not supported`);
    case 'plan-edits':
      return createPluginSuccess(request, { edits: [] });
  }
});
```

The source module must ultimately be bundled into one ESM file. Semifold rejects
runtime imports and validates every response, path, dependency, hash, and diagnostic
before applying plugin output.
