<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./docs/public/logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./docs/public/logo-light.svg">
    <img alt="Semifold Logo" src="./docs/public/logo-light.svg">
  </picture>

# Semifold

English | [中文](README.CN.md)

</div>

Next-generation cross-language monorepo version and release management tool.

Nowadays, cross-language monorepos are becoming more and more common. For example, a monorepo may contain a Rust library and a Node.js package, developers use `napi-rs` to create Rust bindings for Node.js.

`semifold` (CLI: `smif` | `semifold`) helps teams manage versions, changelogs, and package publishing across large cross-language monorepos with **consistency, automation, and zero pain**. Whether you are building libraries, apps, or services across multiple languages, Semifold keeps your release pipeline clean and predictable.

## 📖 Documentation

For detailed usage instructions, please refer to the [official documentation](https://semifold.noctisynth.org).

## ✨ Features

| Feature                             | Description                                                    |
| ----------------------------------- | -------------------------------------------------------------- |
| **Cross-language monorepo support** | Manage versions for Rust / Node.js / more (extensible)         |
| **Changeset-based workflow**        | Clear and traceable version reasoning                          |
| **Automatic version bumping**       | `smif version` reads changes and bumps semver                  |
| **Automated changelogs**            | Generated from commit metadata / changesets                    |
| **One-command publishing**          | Publish multiple packages reliably                             |
| **CI-friendly design**              | `smif ci` gives a stable pipeline for GitHub Actions or others |

## 📌 Status

Languages supported:

- [x] Rust
- [x] Node.js
- [x] Python
- [x] C++
- [ ] Go
- [ ] Java
- [ ] Kotlin

## 🧠 Inspiration

Semifold was inspired by the great work from the following projects:

[Changesets](https://github.com/changesets/changesets) — a simple and elegant changeset-based versioning workflow, mainly focused on JavaScript and npm monorepos.

[Covector](https://github.com/jbolda/covector/) — a flexible multi-target release manager designed to support more complex ecosystems.

## 🤝 Contributing

Contributions are very welcome! Please read the [contributing guidelines](CONTRIBUTING.md) to get started.

## 📄 License

Semifold is licensed under the AGPL-3.0 License.
