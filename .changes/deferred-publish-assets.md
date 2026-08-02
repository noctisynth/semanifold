---
semifold: patch:fix
---

Resolve release asset paths only after package publish commands finish, so generated artifacts are
included. Packages without a changelog are now skipped before registry checks or publish commands.
