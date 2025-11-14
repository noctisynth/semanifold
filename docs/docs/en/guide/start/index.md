# Introduction

Semifold is a next-generation cross-language monorepo versioning and release manager that provides consistent version management and release processes, supporting cross-language monorepos.

Nowadays, cross-language monorepos have become increasingly popular. Often, when the main language of a project cannot meet the project requirements, developers choose to use other languages to implement certain features of the project. For example, a repository might contain a Rust library and a Node.js package, and developers use `napi-rs` to generate Rust bindings for Node.js to improve application performance.

Semifold helps teams manage versions, changelogs, and package publishing across large cross-language monorepos with consistency, automation, and zero pain. Whether you are building libraries, apps, or services across multiple languages, Semifold keeps your release pipeline clean and predictable.

Semifold provides two command-line tools: `smif` and `semifold`. `smif` is an alias for `semifold`, and they provide a consistent command-line interface.

## Why Choose Semifold?

Unlike existing monorepo version release management tools like Changesets and Covector, Semifold focuses on cross-language repositories and provides consistent version management and release processes. Changesets primarily focuses on JavaScript and npm repositories, while Covector is more general but has a strong dependency on the Node.js runtime environment and struggles with complex workspace environments.

Semifold provides workspace resolvers for languages such as Rust, Node.js, Python, etc., automatically handling cross-language dependency relationships in the workspace and ensuring consistency in version management and release processes. For unsupported languages, Semifold provides a flexible extension mechanism that allows developers to configure workspaces themselves.

Since Semifold is written in Rust, it can provide standalone executables without additional runtime environment configuration. In addition, Semifold is also published on [crates.io](https://crates.io/crates/semifold), [Npm Registry](https://www.npmjs.com/package/semifold), and [PyPI](https://pypi.org/project/semifold/), making it convenient for developers to use in different projects and avoid additional environment configuration.
