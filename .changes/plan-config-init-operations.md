---
semifold-engine: minor:feat
semifold: minor:refactor
---

Add immutable plans and application-service entrypoints for initialization, configuration
migration, release-channel updates, and worktree validation.

Keep CLI modules focused on argument parsing, interaction, embedded asset loading, and localized
result rendering while package discovery, configuration construction, validation, and writes are
owned by the engine.
