<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./docs/public/logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./docs/public/logo-light.svg">
    <img alt="Semifold 标志" src="./docs/public/logo-light.svg">
  </picture>

# Semifold

[English](README.md) | 中文

统一管理跨语言单仓库中的软件包版本、变更日志与发布。

</div>

Semifold 把不同软件生态的软件包连接成一张包含依赖关系的工作区图。它用可审查的变更集记录发布意图，联动修改相关版本与变更日志，再按照依赖顺序发布软件包；Cargo、npm、Python 打包工具、CMake，以及仓库原有的构建和测试工具仍然负责各自擅长的工作。

## Semifold 管理什么

- **跨生态工作区图**：统一管理稳定的软件包身份、清单依赖，以及“原生库发布后必须重新发布 Node.js 绑定”等显式关系。
- **可审查的发布意图**：Markdown 变更集会在版本真正变化前记录受影响的软件包、语义化版本提升、变更日志分类和面向用户的摘要。
- **一致的版本修改**：`smif status` 预览整座仓库的版本决定；`smif version` 根据同一份计划修改清单、内部依赖要求和变更日志，并执行已配置的钩子。
- **按依赖发布**：软件包仓库预检查、各生态自己的发布命令、GitHub Release、附件和部分失败报告进入同一套发布流程。
- **自动维护发布**：存在变更集时，`smif ci` 维护发布分支和拉取请求；该拉取请求合入后，再发布已经准备好的版本。
- **可扩展的软件生态**：仓库内 JavaScript 插件可以在受能力限制的运行环境中提供软件包发现、信息读取和版本修改规划。

目前内置 Rust、Node.js、Python 和 C++ 适配器。每个适配器支持的清单和依赖行为请查看[工作区文档](https://semifold.noctisynth.org/zh/docs/workspace/package-discovery/)。

## 安装

安装后可以使用 `smif` 和 `semifold` 两个等价的命令名称；本文统一使用较短的 `smif`。

### 安装脚本

macOS 与 Linux：

```bash
curl -L https://semifold.noctisynth.org/install/install.sh | sh
```

Windows PowerShell：

```powershell
irm https://semifold.noctisynth.org/install/install.ps1 | iex
```

### Cargo

```bash
cargo install semifold
```

### npm

`@semifold/cli` 完成首次 npm 发布后，可以执行：

```bash
npm install --global @semifold/cli
```

npm 发行包要求 Node.js 20 或更高版本。它通过 napi-rs 为 macOS、Windows 和基于 glibc 的 Linux 提供 x64/arm64 平台包。只需安装 `@semifold/cli`，npm 会选择匹配的原生包。

通过下面的命令验证任意一种安装方式：

```bash
smif --version
```

安装方式和平台边界请查看[安装指南](https://semifold.noctisynth.org/zh/docs/getting-started/installation/)。

## 完成第一次发布

在 Git 仓库根目录执行：

```bash
smif init
smif commit
smif status
```

`smif init` 发现软件包并创建 `.changes/config.toml`。完成一项用户可见的代码改动后，`smif commit` 在 `.changes/*.md` 中记录发布意图，`smif status` 则在修改文件前展示所有直接或间接受影响的软件包。

使用生成的 GitHub Actions 工作流时，把代码与变更集一起提交并合入 `main`。Semifold 随后执行：

```text
代码改动 + 变更集
  -> 发布计划
  -> 自动生成的发布拉取请求
  -> 审查版本与变更日志
  -> 按依赖顺序发布
```

没有使用生成的工作流时，可以依次通过 `smif version --dry-run`、`smif version`、`smif publish --dry-run` 和 `smif publish` 预览并手工执行同一套流程。

在生产仓库中采用这套流程前，建议先完成[第一次发布教程](https://semifold.noctisynth.org/zh/docs/getting-started/first-release/)。

## 文档

- [中文文档](https://semifold.noctisynth.org/zh/docs/)
- [English documentation](https://semifold.noctisynth.org/docs/)
- [配置参考](https://semifold.noctisynth.org/zh/docs/configuration/reference/)
- [CLI 命令参考](https://semifold.noctisynth.org/zh/docs/commands/reference/)
- [插件系统](https://semifold.noctisynth.org/zh/docs/plugins/overview/)

## 参与贡献

欢迎提交问题和拉取请求。修改行为、配置、公开文档或软件包元数据前，请先阅读[中文贡献指南](CONTRIBUTING.CN.md)。

## 致谢

Semifold 的设计受到 [Changesets](https://github.com/changesets/changesets) 和 [Covector](https://github.com/jbolda/covector/) 启发，并将这套工作流扩展到跨越多种软件包生态的依赖图。

## 许可证

Semifold 使用 [AGPL-3.0-only 许可证](LICENSE)发布。
