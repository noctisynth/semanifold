semifold: patch:fix
semifold-engine: patch:fix
---

Report post-version commands in their actual sequential execution order. Commands with captured
output show per-command progress, while commands inheriting the terminal only print a result after
their child process exits.
