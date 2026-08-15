# 参与 Semifold 开发

[English](CONTRIBUTING.md) | 中文

Semifold 欢迎问题报告、设计讨论、文档改进和代码贡献。因为一项改动可能同时影响多种软件包生态和发布入口，贡献者需要明确说明预期行为、验证结果与发布影响。

## 开始之前

- 提交新问题前先搜索[现有问题](https://github.com/noctisynth/semifold/issues)。
- 修改行为或架构前，先阅读权威的 [Rust 架构 PRD](docs/prd/rust-architecture-redesign.md)。如果目标设计发生变化，先更新 PRD，再开始实现。
- [TODO.md](TODO.md) 只记录已接受 PRD 与当前代码之间的差异，不是独立的需求来源。
- 一个拉取请求应聚焦于一项可以独立审查的任务。大型或含义不明确的设计变更，建议先通过 issue 讨论，再投入完整实现。

## 开发环境

- Git；
- 当前稳定版 Rust 工具链与 Cargo；
- 根工作区声明的 Bun canary；
- Node.js 20 或更高版本，用于 npm CLI 与 N-API 检查；
- 可选：[prek](https://prek.j178.dev/installation/)，用于本地 pre-commit hooks。

Fork 仓库后，克隆自己的 fork 并安装 JavaScript 工作区依赖：

```bash
git clone https://github.com/<your-account>/semifold.git
cd semifold
git remote add upstream https://github.com/noctisynth/semifold.git
bun install --frozen-lockfile
cargo build --workspace
```

从最新的 `main` 创建分支：

```bash
git switch main
git pull --ff-only upstream main
git switch -c <type>/<short-description>
```

如需安装可选 hooks，请执行：

```bash
prek install
```

## 仓库结构

| 路径 | 职责 |
| --- | --- |
| `crates/semifold-core` | 纯工作区图、发布规划和共享领域类型 |
| `crates/semifold-engine` | 应用服务，以及受控的文件系统、Git、Forge 和子进程副作用 |
| `crates/resolver` | 内置软件生态适配器，以及 JavaScript 插件协议与运行环境 |
| `crates/changelog` | 变更日志读取与渲染 |
| `crates/semifold` | `smif`/`semifold` CLI、终端渲染、本地化和工作流入口 |
| `crates/semifold-napi` | 向 Node.js 暴露 Rust CLI 的 N-API 入口 |
| `packages/cli` | `@semifold/cli`、napi-rs 生成的加载器、打包检查和 Node smoke test |
| `packages/plugin-sdk` | 面向软件生态插件的公开 TypeScript 类型与辅助函数 |
| `docs` | 英文与中文文档站 |
| `.changes` | Semifold 配置和可审查的发布变更集 |

## 实现改动

### 保持设计边界

- 领域计算与 I/O、子进程执行保持分离。渲染上下文必须不可变，并且只服务于对应作用域；不要引入全局可变的万能上下文。
- 对外部输入、配置、文件系统和领域查询，生产 Rust 代码必须返回可恢复错误。不要新增会 panic 的 `unwrap()`。只有当前类型或穷尽控制流已经证明内部不变量时才能使用 `expect()`，消息中必须说明这个不变量。
- 不要手工编辑任何 `Cargo.toml`。使用 `cargo add`、`cargo remove`、`cargo new` 或 `cargo init` 等 Cargo 命令。如果 Cargo 无法表达所需清单变更，应停止并讨论限制，不能绕过规则。
- 修改项目清单或 `.changes/config.toml` 时，保留与任务无关的用户格式和字段。

### 同步用户文案与文档

- 生产 CLI 中的消息、描述、提示和错误都必须通过 `rust-i18n` 提供。
- 同时更新 `crates/semifold/locales/en.toml` 与 `crates/semifold/locales/zh.toml`，两者的 key 集合必须完全一致。
- 行为、配置、CLI/API、工作流、示例或架构说明变化时，同时更新英文与中文公开文档。
- 文档只描述已经验证的行为。已经进入 `main`、但还没有发布的能力，需要在面向发布版本的文档中明确标注。

### 发布面变化时添加 changeset

影响已发布软件包的功能、修复、重构、依赖和测试能力变更，都需要在 `.changes/` 中新增文件。使用本地 CLI 交互创建：

```bash
cargo run -p semifold --bin smif -- commit
```

软件包 ID 必须与 `.changes/config.toml` 中的配置完全一致，版本提升和变更日志 tag 也要符合改动性质。每项独立任务都应拥有自己的 changeset。

只修改内容、不影响任何已发布软件包的文档与仓库维护任务不需要 changeset。文档工具、构建、测试或 `@semifold/docs` 行为变化时仍然需要。

创建 changeset 后，检查发布计划：

```bash
cargo run -p semifold --bin smif -- status
```

changeset 应与实现一起提交，让审查者能在版本变化前判断发布影响。正常自动发布流程中，贡献者不应自行运行 `smif version` 或 `smif publish`。

## 验证

开发过程中先运行与改动相关的检查。仓库范围的代码改动在请求审查前，应执行与 CI 一致的核心检查：

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace --verbose
bun run --filter @semifold/plugin-sdk check
bun run --filter @semifold/plugin-sdk test
bun run --filter @semifold/cli check
```

修改 N-API 或 npm CLI 时，还要为当前主机构建并实际加载 binding：

```bash
bun run --filter @semifold/cli build:debug
bun run --filter @semifold/cli test:native
git diff --exit-code -- packages/cli/index.js packages/cli/index.d.ts
```

本地 N-API 验证只覆盖当前主机。GitHub Actions target matrix 负责 macOS、Windows 和基于 glibc 的 Linux 上 x64/arm64 产物。

修改文档站时执行：

```bash
bun run docs:check
```

如果无法完成某项完整检查，请在拉取请求中准确说明执行和未执行的内容，不要把没有运行的检查写成已经通过。

## 提交与拉取请求

提交主题遵循 [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)，例如：

```text
feat: add an ecosystem capability
fix: preserve a manifest field
docs: clarify the release workflow
```

提交拉取请求前：

1. 检查 diff 中是否混入生成文件、凭据或无关改动；
2. 确认行为、测试、文档和 changeset 相互一致；
3. 说明技术方案相对既有设计的变化；
4. push 分支，并向 `main` 提交拉取请求。

拉取请求描述应包含：

- 要解决的用户或维护者问题；
- 采用的技术方案与设计权衡；
- 用户可见行为、配置、API 或工作流变化；
- 已运行的测试和检查；
- changeset 与文档影响；如果省略其中一项，也要说明原因。

合入后，由 Semifold 自动维护发布分支、版本变化、变更日志与发布。除非恢复任务明确要求，不要手工修改自动生成的发布拉取请求。

## 许可证

提交贡献即表示你同意按照仓库的 [AGPL-3.0-only 许可证](LICENSE)发布该贡献。
