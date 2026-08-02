---
semifold-resolver: major:refactor
semifold: major:refactor
---

Use kebab-case for every Semifold configuration field.

Snake-case configuration keys are no longer supported. Repository configuration, generated
configuration, and fixtures now use fields such as `dry-run`, `extra-env`, and `extra-headers`.
