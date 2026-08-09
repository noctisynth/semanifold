# Legacy URL inventory

Rspress clean URLs are preserved as static HTML redirect pages. Every redirect includes a canonical target, a visible link, and an HTML refresh so it remains useful without client-side routing.

| Legacy path | English successor | Chinese successor | Migration decision |
| --- | --- | --- | --- |
| `/guide/start/` | `/docs/introduction/` | `/zh/docs/introduction/` | Merge into the new mental-model introduction |
| `/guide/start/quick-start/` | `/docs/getting-started/first-release/` | `/zh/docs/getting-started/first-release/` | Replace with the complete first-release tutorial |
| `/guide/commands/init/` | `/docs/reference/cli/` | `/zh/docs/reference/cli/` | Consolidate into the checked CLI surface until the dedicated reference page is written |
| `/guide/commands/commit/` | `/docs/reference/cli/` | `/zh/docs/reference/cli/` | Consolidate into CLI reference |
| `/guide/commands/status/` | `/docs/reference/cli/` | `/zh/docs/reference/cli/` | Consolidate into CLI reference |
| `/guide/commands/version/` | `/docs/reference/cli/` | `/zh/docs/reference/cli/` | Consolidate into CLI reference |
| `/guide/commands/publish/` | `/docs/reference/cli/` | `/zh/docs/reference/cli/` | Consolidate into CLI reference |
| `/guide/commands/ci/` | `/docs/reference/cli/` | `/zh/docs/reference/cli/` | Consolidate into CLI reference |
| `/guide/commands/mcp/` | `/docs/reference/cli/` | `/zh/docs/reference/cli/` | Consolidate into CLI reference until automation/MCP is rewritten |
| `/guide/configuration/config-file/` | `/docs/introduction/` | `/zh/docs/introduction/` | Temporary successor until the configuration reference is rewritten |
| `/guide/configuration/resolvers/` | `/docs/introduction/` | `/zh/docs/introduction/` | Temporary successor until workspace discovery is rewritten |
| `/guide/advanced/changeset-format/` | `/docs/introduction/` | `/zh/docs/introduction/` | Temporary successor until the changeset reference is rewritten |

The same mappings are emitted for `/en/guide/...` to cover deployments that exposed the former default-locale prefix. `/index`, `/en`, `/en/index`, and `/zh/index` redirect to their canonical homepages.
