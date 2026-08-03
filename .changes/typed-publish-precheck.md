semifold: minor:feat
semifold-engine: minor:feat
semifold-resolver: minor:feat
---

Limit Semifold configuration to TOML and add typed HTTP or command publish pre-checks. Command
pre-checks exchange package metadata and existence results through a strict JSON Lines protocol,
while HTTP checks now fail safely on statuses other than 200 and 404.
---
