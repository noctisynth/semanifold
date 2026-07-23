# Changelog

## v0.1.0-alpha.3

### New Features

- [`44ee660`](https://github.com/noctisynth/semifold/commit/44ee660cf4a0993bcb87cea2e43e9f391aa7119d): Plan and atomically apply Rust and Node manifest version edits through the shared release plan.

### Bug Fixes

- [`970de01`](https://github.com/noctisynth/semifold/commit/970de019abd60f6ff889b8520c76b46e51044eba): Define planned file hashes as SHA-256 digests of the source bytes for reliable edit validation.

## v0.1.0-alpha.2

### New Features

- [`cb8c00f`](https://github.com/noctisynth/semifold/commit/cb8c00fd2242bbf7d339f16128c74f360ce4c20e): Add the deterministic config sync plan and pure package drift classifier.
- [`a9a0ac9`](https://github.com/noctisynth/semifold/commit/a9a0ac9c59b99eddb2ee0694b9629d0a115658c8): Bridge configured packages, shared discovery, and pending changesets into config sync planning, including warnings for changesets that reference renamed packages.

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
