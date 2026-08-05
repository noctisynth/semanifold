# Changelog

## v0.2.0-rc.2

### New Features

- [`7ca5d48`](https://github.com/noctisynth/semifold/commit/7ca5d48f26c5de2f888d89c4b45d8650165b9b74): Limit Semifold configuration to TOML and add typed HTTP or command publish pre-checks. Command
    pre-checks exchange package metadata and existence results through a strict JSON Lines protocol,
    while HTTP checks now fail safely on statuses other than 200 and 404.

## v0.2.0-rc.1

### Bug Fixes

- [`b4871b3`](https://github.com/noctisynth/semifold/commit/b4871b3749d814ddb9b7ca38e84c0e2f46d78e37): Stop forcing Cargo offline mode in the Rust resolver's default post-version lockfile generation command.

### New Features

- [`2f843f3`](https://github.com/noctisynth/semifold/commit/2f843f38b196bf61cffd41daa6ae07cdccd6fe94): Allow workspace owners to customize complete changelog release blocks and individual changeset entries with strict MiniJinja templates. Template contexts now expose structured summaries and commit metadata, while stable release markers let publish consume arbitrary rendered formats without breaking legacy changelogs.
- [`42f88c9`](https://github.com/noctisynth/semifold/commit/42f88c9302f8d974c8351aaa30297b0aff2fb002): Write the built-in release and changeset changelog templates into newly initialized configuration files so users can discover and customize them immediately, while retaining the same templates as fallbacks for older configurations.
- [`145ccec`](https://github.com/noctisynth/semifold/commit/145ccecb196b51bbe40d1123c6b7ad6f4678aa25): Add an optional per-package `github-release` policy. Public packages keep GitHub Releases enabled by
    default, private packages keep them disabled by default, and either default can now be overridden
    explicitly without changing registry publishability.

## v0.2.0-rc.0

### New Features

- [`d50a156`](https://github.com/noctisynth/semifold/commit/d50a156035a6520442c0bbd44923d5ac2b36f6b1): Move project loading, configuration synchronization, release planning, changelog preparation, and
    release application behind `SemifoldService` and the new `semifold-engine` boundary.

    CLI and CI now share an immutable `ReleasePlan` followed by a complete `ReleaseApplyPlan`, MCP no
    longer changes the process working directory, and the legacy global mutable `Context` is removed.

- [`c711072`](https://github.com/noctisynth/semifold/commit/c711072eb5c1a67c164dd13cc6e78b4ab09bd26e): Build complete publish plans with project, changelog, and Forge release facts before execution.

    CLI and CI now share the same publish service, while CLI and MCP use one validated changeset
    creation service instead of duplicating resolver and filesystem operations.

- [`a05f89a`](https://github.com/noctisynth/semifold/commit/a05f89a5717b2932d850277d497b036533439036): Add immutable plans and application-service entrypoints for initialization, configuration
    migration, release-channel updates, and worktree validation.

    Keep CLI modules focused on argument parsing, interaction, embedded asset loading, and localized
    result rendering while package discovery, configuration construction, validation, and writes are
    owned by the engine.

- [`3f71dbc`](https://github.com/noctisynth/semifold/commit/3f71dbc2bae9f069769c87364e716c5d73cd263d): Introduce explicit project discovery and loading models with structured errors, including lossless
    handling of non-UTF-8 operating-system paths.


### Refactors

- [`6838d72`](https://github.com/noctisynth/semifold/commit/6838d72730fc38e389885322637621bba0d2aadd): Introduce explicit domain and application error boundaries and remove `anyhow` from the engine.

    All production targets now reject panic-prone unwraps, expects, indexing, and slicing under strict
    Clippy validation, including workspace planning, Rust manifest edits, changelog metadata parsing,
    configuration editing, and embedded initialization assets.
