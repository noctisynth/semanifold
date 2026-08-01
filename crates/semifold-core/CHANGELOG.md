# Changelog

## v0.1.0-beta.1

### New Features

- [`e8f8a09`](https://github.com/noctisynth/semifold/commit/e8f8a0966142aacdab13a33d41332fd9612e4cf9): Add one-shot channel transition bump overrides to `config channel set`, including a preserve mode for entering prerelease channels without raising the stable version base.

## v0.1.0-beta.0

### Bug Fixes

- [`fd19bf0`](https://github.com/noctisynth/semifold/commit/fd19bf09d523d07f75813c064b97b8ca899d2d64): Preserve configured package IDs when manifests share names across ecosystems.

### New Features

- [`e108ed0`](https://github.com/noctisynth/semifold/commit/e108ed0721d928d8ca543ce7bc3c00db0030afe3): Support explicit cross-ecosystem dependency ordering and release propagation.

### Refactors

- [`1932935`](https://github.com/noctisynth/semifold/commit/1932935da7bd936f07c4f3bc58b01e9552350994): Document and enforce dependency-kind ordering and propagation policies.

## v0.1.0-alpha.5

### Bug Fixes

- [`fce12df`](https://github.com/noctisynth/semifold/commit/fce12dff6bf2282af11f9129d3620ef852daf1e8): Plan Rust package and workspace dependency version edits as one deterministic batch.

## v0.1.0-alpha.4

### Bug Fixes

- [`3b961e0`](https://github.com/noctisynth/semifold/commit/3b961e069ea4bd5b43e5b29bf2f5c5fc39414c9b): Return errors instead of panicking when release planning, configuration, changelog, and resolver invariants are unavailable.

### Refactors

- [`f42b195`](https://github.com/noctisynth/semifold/commit/f42b195058a4f6972f1a7468023c0512f4845d24): Keep recoverable input failures as errors while using documented `expect` calls for verified internal invariants.
- [`56e0686`](https://github.com/noctisynth/semifold/commit/56e06863fb2846497ed0b79417d68f3cf17eb8ca): Enforce the production unwrap policy through shared workspace Clippy configuration while allowing documented internal expects.

### New Features

- [`09e7af6`](https://github.com/noctisynth/semifold/commit/09e7af6e76bed7a01b72e6a675504ab308732d2f): Plan Rust and Node changelog updates as validated file edits that can safely create a missing changelog.

## v0.1.0-alpha.3

### Bug Fixes

- [`970de01`](https://github.com/noctisynth/semifold/commit/970de019abd60f6ff889b8520c76b46e51044eba): Define planned file hashes as SHA-256 digests of the source bytes for reliable edit validation.

### New Features

- [`44ee660`](https://github.com/noctisynth/semifold/commit/44ee660cf4a0993bcb87cea2e43e9f391aa7119d): Plan and atomically apply Rust and Node manifest version edits through the shared release plan.

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
