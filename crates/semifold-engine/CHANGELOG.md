# Changelog

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

