---
"@semifold/docs": minor:feat
---

Allow the installation scripts to install Semifold into a custom directory

Unix users can pass `--install-dir`, while Windows users can pass `-InstallDir`. Both scripts keep
using the user-local binary directory by default and allow a custom directory to be combined with
a specific version.
