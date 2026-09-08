---
name: semifold
description: Use Semifold (smif) to adopt release management, create changesets, inspect release plans, maintain package configuration, or diagnose version and publish workflows in a user's repository.
---

# Semifold

Use the repository's installed `smif` or `semifold` executable. Run `smif --version` and the relevant command's `--help` to check the installed surface before selecting flags. If the binary is unavailable, follow the [installation guide](https://semifold.noctisynth.org/docs/getting-started/installation/); do not compile Semifold or change the installation method when that conflicts with the user's constraints.

## Inspect and choose a task

Read the target repository's instructions, Git diff, `.changes/config.toml` (or its configured changeset directory), and release workflow. Package arguments are the exact keys in `[packages]`, not guessed registry names. Inspect configured hooks before previewing version or publish operations.

- **New adoption:** use `smif init`. In a non-interactive environment, explicitly choose ecosystems, distinct base/release branches, tags or no tags, and GitHub Actions or no GitHub Actions. Consult [initialization](https://semifold.noctisynth.org/docs/commands/init/).
- **Existing repository maintenance:** use `smif config sync --check` to inspect package drift, then `smif config sync` when applying the intended updates. Reinitializing with `init --force` is not routine synchronization. Consult [configuration commands](https://semifold.noctisynth.org/docs/commands/config/).
- **Record a change:** read the affected package IDs and configured tags, then create one changeset for the requested change. A complete non-interactive example, after substituting actual values, is `smif commit --name fix-parser --package core=patch --no-tag --summary "Fix parser handling of empty input."`. Use `--tag` only for a configured tag. Consult [changesets](https://semifold.noctisynth.org/docs/commands/commit/).
- **Review release intent:** run `smif status` and explain direct changeset effects and propagated package bumps. A successful status does not mean versions were applied or packages published. `status --comment` is a GitHub write operation.
- **Version, publish, or diagnose CI:** read [release workflow](references/release-workflow.md) before choosing the automated or manual path.

## Execution boundaries

Preserve the user's choice of workflow and existing authorization. Creating a changeset does not imply pushing, merging, or publishing. Do not change branch protection, credentials, release branches, or package policy merely to make a failed operation pass.

`--dry-run` skips Semifold-managed mutations, but explicitly dry-run-enabled configured commands can still execute, and publish preflight can access registries. Do not describe every preview as side-effect-free. Respect local build and network constraints.

After creating a changeset, run `smif status` and report affected packages, target versions, propagation, and remaining verification. For mutation tasks, inspect the resulting Git diff. For failures, report the operation and available diagnostic, then use the operation-specific recovery guidance rather than deleting changesets or repeatedly publishing.

## Read only the references needed

The public MDX documentation is the product reference; this skill supplies task routing rather than a second command manual. Discover focused Markdown pages through the [English index](https://semifold.noctisynth.org/llms.txt) or [中文索引](https://semifold.noctisynth.org/zh/llms.txt). The [command reference](https://semifold.noctisynth.org/docs/commands/reference/) lists the supported commands. Check installed help when documentation and an older binary differ.

For Chinese requests, respond in Chinese and use the Chinese index. 本技能用于在使用者仓库中操作 Semifold；修改 Semifold 自身源码时，遵循其仓库 AGENTS.md。
