---
semifold-core: minor:feat
semifold-changelog: minor:feat
semifold: minor:refactor
---

Model changelog rendering as immutable package and changeset facts.

Changelog collection now resolves package sections and optional commit and pull request metadata
before passing a capability-free aggregate context to the Markdown formatter.
