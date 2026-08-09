---
"@semifold/plugin-sdk": minor:feat
semifold-resolver: patch:fix
semifold: patch:fix
---

Introduce the versioned TypeScript SDK for ecosystem plugins with exact schema v1 wire types,
construction helpers, and declarations limited to the capabilities provided by the Boa runtime.

Build and validate the public package without Node.js or DOM ambient types, share JSON contract
fixtures with the Rust protocol tests, and prepare OIDC-only automated npm publishing after the
initial version is published locally.
