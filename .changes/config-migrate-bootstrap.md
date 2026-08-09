---
semifold: "patch:fix"
semifold-engine: "patch:fix"
---

Run config migrate against raw TOML before strict project loading, and warn when channel updates target Node.js packages whose npm publish command lacks an explicit dist-tag.
