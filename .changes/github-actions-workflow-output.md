semifold: minor:feat
semifold-engine: minor:feat
---

Expose versioned, allowlisted GitHub Actions outputs from `smif version` and `smif publish`.
Version outputs preserve the release plan fingerprint, branch, and package versions, while publish
outputs retain complete package recovery states after partial failures without leaking command or
environment configuration.
