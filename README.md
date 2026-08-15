<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./docs/public/logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./docs/public/logo-light.svg">
    <img alt="Semifold logo" src="./docs/public/logo-light.svg">
  </picture>

# Semifold

English | [中文](README.CN.md)

Version and release management for monorepos that span multiple languages and package ecosystems.

</div>

Semifold connects packages from different ecosystems into one dependency-aware workspace. It records release intent in reviewable changesets, updates related versions and changelogs together, and publishes packages in dependency order without replacing Cargo, npm, Python packaging, CMake, or your existing build and test tools.

## What Semifold manages

- **One cross-ecosystem workspace graph:** stable package identities, manifest dependencies, and explicit relationships such as a native library that must republish its Node.js binding.
- **Reviewable release intent:** Markdown changesets describe affected packages, semantic-version bumps, changelog categories, and user-facing summaries before versions move.
- **Consistent versioning:** `smif status` previews the repository-wide decision; `smif version` applies manifest edits, internal requirement updates, changelogs, and configured hooks from the same plan.
- **Dependency-ordered publishing:** registry pre-checks, ecosystem-specific commands, GitHub Releases, assets, and partial-failure reporting share one publish workflow.
- **Release automation:** `smif ci` maintains a release branch and pull request when changesets exist, then publishes the prepared versions after that pull request is merged.
- **Extensible ecosystems:** repository-local JavaScript plugins can add package discovery, inspection, and version-edit planning within a capability-restricted runtime.

Built-in adapters currently cover Rust, Node.js, Python, and C++. See the [workspace documentation](https://semifold.noctisynth.org/docs/workspace/package-discovery/) for the exact manifest and dependency behavior of each adapter.

## Installation

The installed command is available as both `smif` and `semifold`; the examples use `smif`.

### Installation script

macOS and Linux:

```bash
curl -L https://semifold.noctisynth.org/install/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://semifold.noctisynth.org/install/install.ps1 | iex
```

### Cargo

```bash
cargo install semifold
```

### npm

After the initial npm publication of `@semifold/cli` is complete:

```bash
npm install --global @semifold/cli
```

The npm distribution requires Node.js 20 or newer. It uses napi-rs platform packages for x64 and arm64 on macOS, Windows, and glibc-based Linux. Install only `@semifold/cli`; npm selects the matching native package.

Verify any installation with:

```bash
smif --version
```

For installation boundaries and alternatives, read the [installation guide](https://semifold.noctisynth.org/docs/getting-started/installation/).

## A first release

From the root of a Git repository:

```bash
smif init
smif commit
smif status
```

`smif init` discovers packages and creates `.changes/config.toml`. After a user-visible code change, `smif commit` records its release intent in `.changes/*.md`, and `smif status` shows every directly or transitively affected package before files are changed.

For repositories using the generated GitHub Actions workflows, commit the code and changeset together and merge them into `main`. Semifold then follows this lifecycle:

```text
code change + changeset
  -> release plan
  -> generated release pull request
  -> version and changelog review
  -> dependency-ordered publish
```

Without the generated workflow, preview and apply the same stages manually with `smif version --dry-run`, `smif version`, `smif publish --dry-run`, and `smif publish`.

Follow the [first-release tutorial](https://semifold.noctisynth.org/docs/getting-started/first-release/) before adopting the workflow in a production repository.

## Documentation

- [English documentation](https://semifold.noctisynth.org/docs/)
- [中文文档](https://semifold.noctisynth.org/zh/docs/)
- [Configuration reference](https://semifold.noctisynth.org/docs/configuration/reference/)
- [CLI command reference](https://semifold.noctisynth.org/docs/commands/reference/)
- [Plugin system](https://semifold.noctisynth.org/docs/plugins/overview/)

## Contributing

Issues and pull requests are welcome. Read the [contributing guide](CONTRIBUTING.md) before changing behavior, configuration, public documentation, or package metadata.

## Acknowledgements

Semifold draws inspiration from [Changesets](https://github.com/changesets/changesets) and [Covector](https://github.com/jbolda/covector/), while extending the workflow around a dependency graph that spans package ecosystems.

## License

Semifold is distributed under the [AGPL-3.0-only license](LICENSE).
