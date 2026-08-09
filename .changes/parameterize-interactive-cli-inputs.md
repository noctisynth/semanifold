---
semifold: minor:feat
---

Make every init and changeset creation prompt optional through equivalent CLI arguments so the same
commands run predictably in CI/CD and constrained agent environments with stdin closed. Missing
arguments now fail fast with actionable guidance when prompting is unavailable.
