# Changelog

## v0.1.0-alpha.1

### Bug Fixes

- [`3df4571`](https://github.com/noctisynth/semifold/commit/3df4571d44bcf14d6005d928e5b95b0175586ff8): Format dependency cycle errors as complete arrow-separated package paths.

### New Features

- [`3f09ba4`](https://github.com/noctisynth/semifold/commit/3f09ba473d8e4622d5adda6236193eef20fe7fad): Add the pure release planner with deterministic changeset merging, channel-aware versioning, and constraint-aware dependency propagation.
- [`5cd26b6`](https://github.com/noctisynth/semifold/commit/5cd26b6dff15866cd7901ed4047d3b6d0f544177): Add the immutable release plan, package release, version map, changeset reason, warning, and planned file edit domain models.

## v0.1.0-alpha.0

### New Features

- [`d95c07a`](https://github.com/noctisynth/semifold/commit/d95c07aaa8e930ec6499c020cccf80896ca623bf): Introduce cross-ecosystem package identities and a deterministic workspace dependency graph.
- [`5bea426`](https://github.com/noctisynth/semifold/commit/5bea42624aa7ba73034ad0fba8dc1c9c14da7419): Bridge configured Rust, Node.js, Python, and C++ packages into the new cross-ecosystem workspace graph.

### Bug Fixes

- [`5b9a286`](https://github.com/noctisynth/semifold/commit/5b9a2861befcac4ca466b064a334d6c9bf17b261): Set the new core crate's initial manifest version to `0.0.0` so its first minor release targets `0.1.0`.
