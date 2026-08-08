semifold: patch:fix
---

Align CLI fact columns by Unicode display width and suspend dynamic progress while post-version
commands inherit the terminal, preventing spinner redraws from overwriting child-process output.
