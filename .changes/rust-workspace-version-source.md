---
semifold-core: minor:feat
semifold-resolver: minor:feat
semifold: minor:feat
---

Support Rust packages that inherit `workspace.package.version`.

Shared version sources now merge bumps across every inheriting crate, validate channel policy, keep
private crates in the version closure, and update the owning workspace manifest exactly once.
