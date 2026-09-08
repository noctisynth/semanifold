# Choose the release workflow / 选择发布流程

## English

Check the repository's workflow before running a release command. When generated GitHub Actions are used, contributors create a changeset and inspect `smif status`. Authorized pushes and merges let `smif ci` maintain the release PR; merging that PR triggers publishing. Do not also version or publish locally for the same release.

For an explicitly manual workflow, preview `smif version --dry-run`, review configured hooks, and apply `smif version` within the requested scope. Verify the file diff and repository checks before committing generated versions. Only then preview and execute publishing when authorized. The release branch must differ from the base branch because CI force-maintains it.

Use these existing references for the details and recovery rules:

- [Version](https://semifold.noctisynth.org/docs/commands/version/): post-version hook failure can leave applied edits and retained changesets; inspect and repair before retrying.
- [Publish](https://semifold.noctisynth.org/docs/commands/publish/): inspect succeeded, failed, skipped, and not-started packages. An existing registry version is skipped; an existing GitHub Release does not trigger missing-asset upload recovery.
- [CI](https://semifold.noctisynth.org/docs/commands/ci/): distinguish changeset-based release PR preparation from publishing already prepared versions.
- [Status](https://semifold.noctisynth.org/docs/commands/status/): PR comment write failure is a warning; it does not invalidate the plan. Other GitHub errors include operation and API details when available. A 403 alone does not establish the underlying cause.

## 中文

先检查仓库采用的发布工作流。使用生成的 GitHub Actions 时，贡献者创建 changeset 并查看 `smif status`；在用户授权范围内推送和合并后，由 `smif ci` 维护发布 PR，合并发布 PR 后触发发布。同一次发布不再额外执行本地 version 或 publish。

明确采用手动流程时，先检查配置的 hook 并运行 `smif version --dry-run`，再按任务范围执行 `smif version`。核对文件差异和仓库检查后提交版本变更，仅在授权范围内预览和执行 publish。CI 会强制维护发布分支，因此发布分支必须不同于基础分支。

具体行为与恢复方式以现有文档为准：

- [版本更新](https://semifold.noctisynth.org/zh/docs/commands/version/)：post-version hook 失败可能保留已应用文件和 changeset，先检查并修复再重试。
- [发布](https://semifold.noctisynth.org/zh/docs/commands/publish/)：根据成功、失败、跳过、未开始状态恢复。registry 版本已存在时跳过发布；GitHub Release 已存在时不会自动补传附件。
- [CI](https://semifold.noctisynth.org/zh/docs/commands/ci/)：区分有 changeset 时准备发布 PR 和无 changeset 时发布已准备版本。
- [状态](https://semifold.noctisynth.org/zh/docs/commands/status/)：PR 评论写入失败为警告，不影响已有计划。GitHub 诊断中的操作、HTTP 状态和具体消息共同用于定位原因，不能仅凭 403 断言根因。
