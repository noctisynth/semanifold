# Changelog

## v0.1.1

### Chores

- [`05a2e7c`](https://github.com/noctisynth/semifold/commit/05a2e7c6b85697cbd091f74698d88a87f8678f01): Upgrade Rust and JavaScript dependencies to their latest compatible versions, including the required API migrations for Git, HTTP, hashing, localization, MCP, and configuration handling.

## v0.1.0

### Chores

- [`28ee39d`](https://github.com/noctisynth/semifold/commit/28ee39d4295b91d1afc8614eec0ed6327ca22af7): Promote the remaining prerelease packages to their stable release channel.

## v0.1.0-rc.1

### Chores

- [`9632bac`](https://github.com/noctisynth/semifold/commit/9632bacee5412b6a37103a68b8db1e7b8e86909a): Migrate the repository JavaScript workspace, SDK checks, runtime tests, documentation build, and CI/CD dependency installation from pnpm to Bun canary with a committed `bun.lock`.

## v0.1.0-rc.0

### Bug Fixes

- [`5b6694b`](https://github.com/noctisynth/semifold/commit/5b6694bbd17d86f9f3b44977ea402849d26228a3): Format generated plugin SDK bindings with Biome before writing or drift checks, and enforce SDK formatting in CI.
- [`6b451f6`](https://github.com/noctisynth/semifold/commit/6b451f614df71cb1618c81dc02ff3e37953e4c6f): Generate plugin SDK wire types from the Rust serde protocol, preserve deeply readonly public types, and fail CI when committed bindings drift.

    Serialize workspace-manifest edit source fields with the documented kebab-case names.


### New Features

- [`0c59f66`](https://github.com/noctisynth/semifold/commit/0c59f66364e6617e9e711c21f217dbff256ffcdf): Introduce the versioned TypeScript SDK for ecosystem plugins with exact schema v1 wire types, construction helpers, and declarations limited to the capabilities provided by the Boa runtime.

    Build and validate the public package without Node.js or DOM ambient types, share JSON contract fixtures with the Rust protocol tests, and prepare OIDC-only automated npm publishing after the initial version is published locally.
