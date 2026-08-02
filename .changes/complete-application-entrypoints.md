---
semifold-engine: minor:feat
semifold: minor:refactor
---

Build complete publish plans with project, changelog, and Forge release facts before execution.

CLI and CI now share the same publish service, while CLI and MCP use one validated changeset
creation service instead of duplicating resolver and filesystem operations.
