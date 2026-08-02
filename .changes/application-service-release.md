---
semifold-engine: minor:feat
semifold-core: major:refactor
semifold-resolver: major:refactor
semifold-changelog: major:refactor
semifold: major:refactor
---

Move project loading, configuration synchronization, release planning, changelog preparation, and
release application behind `SemifoldService` and the new `semifold-engine` boundary.

CLI and CI now share an immutable `ReleasePlan` followed by a complete `ReleaseApplyPlan`, MCP no
longer changes the process working directory, and the legacy global mutable `Context` is removed.
