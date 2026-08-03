---
semifold: minor:feat
semifold-engine: minor:feat
semifold-resolver: minor:feat
---

Add an optional per-package `github-release` policy. Public packages keep GitHub Releases enabled by
default, private packages keep them disabled by default, and either default can now be overridden
explicitly without changing registry publishability.
