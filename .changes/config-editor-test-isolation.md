---
semifold: patch:test
---

Isolate config editor tests when the suite runs concurrently.

Temporary configuration paths now include the process ID and an atomic sequence so parallel tests
cannot delete one another's fixtures.
