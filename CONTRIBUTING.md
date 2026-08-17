# Contributing to Semifold

English | [中文](CONTRIBUTING.CN.md)

Semifold welcomes bug reports, design discussions, documentation improvements, and code contributions. Because one change can affect several package ecosystems and release surfaces, contributors should make the intended behavior, validation, and release impact explicit.

## Before you start

- Search [existing issues](https://github.com/noctisynth/semifold/issues) before opening a new report.
- For a behavior or architecture change, read the authoritative [Rust architecture PRD](docs/prd/rust-architecture-redesign.md) first. If the intended design changes, update the PRD before implementation.
- Treat [TODO.md](TODO.md) only as the list of differences between accepted PRDs and the current code. It is not an independent source of requirements.
- Keep a pull request focused on one independently reviewable task. Discuss a large or ambiguous design change in an issue before investing in a full implementation.

## Development prerequisites

- Git;
- the current stable Rust toolchain with Cargo;
- Bun canary, as declared by the root workspace;
- Node.js 20 or newer for the npm CLI and N-API checks;
- optional: [prek](https://prek.j178.dev/installation/) for local pre-commit hooks.

Fork the repository, then clone your fork and install the JavaScript workspace dependencies:

```bash
git clone https://github.com/<your-account>/semifold.git
cd semifold
git remote add upstream https://github.com/noctisynth/semifold.git
bun install --frozen-lockfile
cargo build --workspace
```

Create a branch from an up-to-date `main`:

```bash
git switch main
git pull --ff-only upstream main
git switch -c <type>/<short-description>
```

Install the optional hooks with:

```bash
prek install
```

## Repository map

| Path | Responsibility |
| --- | --- |
| `crates/semifold-core` | Pure workspace graph, release planning, and shared domain types |
| `crates/semifold-engine` | Application services and controlled filesystem, Git, Forge, and process effects |
| `crates/resolver` | Built-in ecosystem adapters and the JavaScript plugin protocol/runtime |
| `crates/changelog` | Changelog inspection and rendering |
| `crates/semifold` | The `smif`/`semifold` CLI, terminal rendering, localization, and workflow entry points |
| `crates/semifold-napi` | N-API entry point that exposes the Rust CLI to Node.js |
| `packages/cli` | `@semifold/cli`, generated napi-rs loader, packaging checks, and Node smoke tests |
| `packages/plugin-sdk` | Public TypeScript types and helpers for ecosystem plugins |
| `docs` | English and Chinese documentation site |
| `.changes` | Semifold configuration and reviewable release changesets |

## Implementing a change

### Preserve the design boundaries

- Keep domain calculations separate from I/O and process execution. Rendering contexts are immutable and scoped to their task; do not introduce a global mutable catch-all context.
- Production Rust code must return recoverable errors for external input, configuration, filesystems, and domain queries. Do not add panic-producing `unwrap()` calls. Use `expect()` only for an invariant proven by the local type or exhaustive control flow, and explain that invariant in its message.
- Do not manually edit any `Cargo.toml`. Use Cargo commands such as `cargo add`, `cargo remove`, `cargo new`, or `cargo init`. If Cargo cannot express a required manifest change, stop and discuss the constraint instead of bypassing it.
- Preserve unrelated user formatting and fields when editing project manifests or `.changes/config.toml`.

### Keep user-facing text and documentation synchronized

- Route every production CLI message, description, prompt, and error through `rust-i18n`.
- Update both `crates/semifold/locales/en.toml` and `crates/semifold/locales/zh.toml`; their key sets must remain identical.
- Update English and Chinese public documentation together whenever behavior, configuration, CLI/API, workflow, examples, or architecture explanations change.
- Document verified behavior only. Capabilities implemented on `main` but not yet published must be labelled accordingly in release-oriented documentation.

### Add a changeset when the release surface changes

Features, fixes, refactors, dependency changes, and test capabilities that affect a published package require a new file in `.changes/`. Create one interactively with the local CLI:

```bash
cargo run -p semifold --bin smif -- commit
```

Select package IDs exactly as configured in `.changes/config.toml`, and choose the bump level and changelog tag that match the change. Each independent task should receive its own changeset.

Pure content-only documentation and repository maintenance that affect no published package do not require a changeset. Changes to documentation tooling, builds, tests, or the behavior of `@semifold/docs` do require one.

After creating a changeset, verify the release plan:

```bash
cargo run -p semifold --bin smif -- status
```

Commit the changeset with the implementation so reviewers can evaluate the release impact before versions move. Contributors should not run `smif version` or `smif publish` for the normal automated release path.

## Validation

Run the checks relevant to your change while developing. Before requesting review for a repository-wide code change, run the same core checks as CI:

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace --verbose
bun run --filter @semifold/plugin-sdk check
bun run --filter @semifold/plugin-sdk test
bun run --filter @semifold/cli check
```

For N-API or npm CLI changes, also build and load the binding for the current host:

```bash
bun run --filter @semifold/cli build:debug
bun run --filter @semifold/cli test:native
git diff --exit-code -- packages/cli/index.js packages/cli/index.d.ts
```

Local N-API validation covers only the current host. The GitHub Actions target matrix is responsible for x64 and arm64 artifacts on macOS, Windows, and glibc-based Linux.

For documentation-site changes, run:

```bash
bun run docs:check
```

If a full check is impractical, state exactly what was and was not run in the pull request. Do not describe an unexecuted check as passing.

## Commits and pull requests

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) for commit subjects, for example:

```text
feat: add an ecosystem capability
fix: preserve a manifest field
docs: clarify the release workflow
```

Before opening a pull request:

1. review the diff for generated files, credentials, and unrelated edits;
2. confirm behavior, tests, documentation, and changeset agree;
3. explain any change from the previously accepted technical design;
4. push the branch and open the pull request against `main`.

The pull request description should include:

- the user or maintainer problem being solved;
- the chosen technical approach and any design tradeoffs;
- user-visible, configuration, API, or workflow changes;
- tests and checks that were run;
- the changeset and documentation impact, including why either is unnecessary when omitted.

After merge, Semifold's automation owns the release branch, version changes, changelogs, and publishing. Do not manually edit the generated release pull request unless recovery work explicitly requires it.

## License

By contributing, you agree that your contribution is distributed under the repository's [AGPL-3.0-only license](LICENSE).
