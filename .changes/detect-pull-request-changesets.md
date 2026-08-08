semifold: patch:fix
---

Detect changesets introduced or changed by the current pull request through the paginated GitHub
files API, list them in status comments, and base the explanatory empty state on that branch scope.
