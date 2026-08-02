---
semifold-core: minor:feat
semifold-engine: minor:refactor
semifold-resolver: patch:fix
semifold-changelog: patch:fix
semifold: patch:fix
---

Introduce explicit domain and application error boundaries and remove `anyhow` from the engine.

All production targets now reject panic-prone unwraps, expects, indexing, and slicing under strict
Clippy validation, including workspace planning, Rust manifest edits, changelog metadata parsing,
configuration editing, and embedded initialization assets.
