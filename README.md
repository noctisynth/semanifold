# Semifold

Next-generation cross-language monorepo version and release management tool.

Nowadays, cross-language monorepos are becoming more and more common. For example, a monorepo may contain a Rust library and a Node.js package, developers use `napi-rs` to create Rust bindings for Node.js.

`semifold` (CLI: `smif` | `semifold`) helps teams manage versions, changelogs, and package publishing across large cross-language monorepos with **consistency, automation, and zero pain**. Whether you are building libraries, apps, or services across multiple languages, Semifold keeps your release pipeline clean and predictable.

## ✨ Features

| Feature                             | Description                                                    |
| ----------------------------------- | -------------------------------------------------------------- |
| **Cross-language monorepo support** | Manage versions for Rust / Node.js / more (extensible)         |
| **Changeset-based workflow**        | Clear and traceable version reasoning                          |
| **Automatic version bumping**       | `smif version` reads changes and bumps semver                  |
| **Automated changelogs**            | Generated from commit metadata / changesets                    |
| **One-command publishing**          | Publish multiple packages reliably                             |
| **CI-friendly design**              | `smif ci` gives a stable pipeline for GitHub Actions or others |

## 🚀 Quick Start

### 1. Install

```bash
cargo install semifold
```

### 2. Initialize config

```bash
smif init
```

### 3. Add a change

```bash
smif commit
```

### 4. Bump versions

```bash
smif version
```

### 5. Publish packages

```bash
smif publish
```

## 📌 Status

Languages supported:

- [x] Rust
- [x] Node.js
- [ ] Python
- [ ] Go
- [ ] Java
- [ ] Kotlin

## 📄 License

Semifold is licensed under the AGPL-3.0 License.
