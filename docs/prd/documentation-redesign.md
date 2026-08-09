# Semifold 文档体验重构方案

## 1. 文档状态

- 状态：Accepted，阶段 1 纵向切片已实现
- 更新时间：2026-08-10
- 范围：文档站技术架构、信息架构、内容规范、双语策略、迁移与质量保障
- 事实来源：本 PRD 定义文档产品；Semifold 的领域与行为仍以
  [Rust 架构重设计方案](rust-architecture-redesign.md) 和已验证实现为准
- 目标发布：文档站跟随当前已发布版本；尚未发布的 `main` 能力必须明确标注，不能伪装成当前稳定用法

### 1.1 文档与变更治理

任何影响文档信息架构、公开路由、双语策略、搜索、部署或内容事实来源的变更，必须先更新本 PRD。
根目录 `TODO.md` 只记录本 PRD 与当前文档站之间尚未完成的差异。

纯内容修改不需要 changeset。迁移框架、依赖、构建、部署或测试能力会影响 `@semifold/docs` package，
必须为该独立任务创建新的 changeset，并使用 `.changes/config.toml` 中的 package 名称。

## 2. 2026-08-10 开发事实基线

### 2.1 发布状态

- 最新 Semifold tag 为 `semifold-v0.3.0-rc.5`。
- 当前开发状态复核到 commit `4e2a40e`，`main` 比该 tag 超前 22 个提交；最新提交只调整本仓库 npm 发布命令的 `rc` dist-tag，不改变 Semifold 的公开命令面。
- 开始文档重构前，release plan 包含 15 个待消费 changeset，计划提升 5 个 package。
- `@semifold/plugin-sdk` 当前 manifest 为 `0.1.0-alpha.0`，其首个 changeset 计划进入 `0.1.0-rc.0`。

文档重构期间，安装与默认工作流必须描述最新已发布版本。面向下一 RC 的页面允许提前编写，但必须满足
下列条件之一：

1. 页面显式标注“下一版本”；
2. 页面在对应 package 发布后才进入生产导航；
3. 示例同时说明已发布版本与 `main` 的差异。

### 2.2 已发布能力

`0.3.0-rc.5` 已包含并可作为默认文档事实：

- Rust、Node.js、Python 与 C++ 内置 ecosystem；
- changeset 创建、合并 bump 与 changelog 分类；
- `status`、`version`、`publish` 和 `ci` 的共享 release/publish plan；
- `config sync`、`config migrate` 和 release channel 管理；
- 跨生态显式 `depends-on`、确定性拓扑排序与依赖环诊断；
- 自定义 changelog release/changeset 模板；
- typed HTTP/command publish pre-check 与 retry；
- GitHub Release、asset 上传与 per-package `github-release` 策略；
- GitHub Actions version/publish 结构化 outputs；
- 可完整参数化的 `init` 与 `commit` 非交互路径；
- TTY/CI、自适应 Unicode 表格、dry-run 与部分失败恢复反馈。

### 2.3 已合入 `main`、等待下一次发布的能力

- 开放 `EcosystemId` 与 repository-local JavaScript ecosystem plugin；
- Boa 单文件 ESM runtime；
- 按 glob 授权的只读文件 capability；
- default-deny、按 exact HTTPS origin 授权的 fetch capability；
- plugin discovery、inspection、plan-edits 与 config sync/version 集成；
- `@semifold/plugin-sdk`、Rust 协议生成的 TypeScript wire types 与 drift 检查；
- MCP `get_changeset`、`create_changeset`、`update_changeset`、`delete_changeset`；
- MCP optimistic SHA-256 revision、dry-run、panic isolation 与 lazy project load；
- strict project load 前执行 `config migrate`，以及 Node.js channel/dist-tag warning；
- registry 版本已存在时继续补建缺失 GitHub Release；
- npm trusted publishing/OIDC 与 init workflow 模板保留修复。

### 2.4 尚未完成、不得写成可用能力

- 配套 Vite plugin 的单文件 ESM bundle；
- 对残留 import、Node.js builtin、动态 module load 和不支持 Web API 的构建期检查。

在该能力完成前，plugin 文档只能要求用户自行提供无 runtime import 的单文件 ESM；不得展示尚不存在的
`vite` 命令或 package。

### 2.5 验证状态

基线审计时：

- `cargo test --workspace --all-features`：346 项通过；
- `@semifold/plugin-sdk` protocol/Biome 检查通过；
- SDK 类型测试与 3 项 runtime 测试通过；
- 现有 Rspress production build 通过；
- `smif config sync --check` 与 `smif config migrate --check` 通过；
- 工作树干净。

## 3. 现状诊断

当前 Rspress 文档按 CLI 表面结构组织，只有 Start、Commands、Configuration 与 Advanced 四类内容。
它没有先建立 release workflow 心智模型，也没有区分 workspace owner、contributor、release engineer 与
plugin author 的任务。

已确认的问题包括：

- Quick Start 在同一页混合安装、初始化、配置、changeset、version、publish 与 CI；
- 用户在理解 Semifold 之前先面对多个安装渠道和完整配置；
- `config`、release channel、workflow outputs、模板与失败恢复等已实现能力缺失；
- 旧配置页仍描述 legacy `version-mode`、snake_case 字段与过期 pre-check 形状；
- MCP 页面仍描述已经删除的 `get_tags`、`get_packages` 和旧 `create_changeset` surface；
- 桌面导航重叠、首页 emoji 缺字、移动端代码被截断；
- 公开页面暴露 `/index` 链接，部分直接访问返回 404；
- 部署只验证 build，不验证链接、双语完整性、静态深链、移动端或内容示例。

迁移不得复制这些页面结构或逐页转换旧 MDX。

## 4. 目标与非目标

### 4.1 主要目标

1. 让首次用户先理解 Semifold 的 release workflow，再接触配置细节。
2. 为 workspace owner、contributor、release engineer 和 plugin author 提供独立成功路径。
3. 以真实 CLI、配置类型、fixture 和 release plan 约束文档事实。
4. 保留英文无前缀与中文 `/zh` 的公共 URL 语义。
5. 在 GitHub Pages 上提供完全静态的双语站点、搜索、SEO 与 LLM 文本入口。
6. 让深层路由、链接、双语覆盖与关键示例进入 CI。
7. 限制定制边界，避免 fork Fumadocs 核心布局造成长期维护负担。

### 4.2 非目标

- 第一阶段不实现需要服务端 runtime 的 Ask AI 或内建 feedback backend；
- 第一阶段不提供历史版本切换器；
- 不从 Rust 内部类型直接生成全部解释性内容；
- 不承诺未发布、未完成或只存在于 PRD 的能力；
- 不把首页改造成与文档无关的独立营销网站。

## 5. 用户与核心任务

### 5.1 Workspace owner

- 判断 Semifold 是否适合现有 monorepo；
- 初始化、检查 package discovery，并维护配置漂移；
- 建立跨生态依赖、发布通道和 changelog 策略；
- 配置 GitHub Actions 与 registry 权限。

### 5.2 Contributor

- 为一次变更创建正确的 changeset；
- 选择 package、bump 与 changelog tag；
- 在提交前读懂 `smif status`。

### 5.3 Release engineer

- 理解 release PR、version、publish 和 registry preflight；
- 消费 GitHub Actions outputs；
- 诊断 partial failure、already-published package 与 Forge/asset 状态。

### 5.4 Plugin author

- 判断内置 ecosystem 是否已经满足需求；
- 编写 schema v1 JavaScript plugin；
- 配置 read patterns、SHA-256 pin 与 HTTPS origins；
- 使用 TypeScript SDK，并自行生成首版要求的单文件 ESM；
- 理解 plugin 不能执行的 privileged operations。

## 6. 核心心智模型

所有 onboarding 和 workflow 内容围绕同一条路径：

```text
code change
  -> changeset
  -> immutable release plan (`smif status`)
  -> version edits and release pull request
  -> dependency-ordered publish plan
  -> registry / Forge report
```

必须在进入完整配置 reference 前解释：

- PackageId 与 manifest name 的区别；
- ecosystem adapter/plugin 负责 discovery、inspection 与 edit planning；
- changeset 表达发布意图，tag 只分类 changelog，不决定 release channel；
- `status` 与 `version` 使用同一个 plan；
- `publish` 使用当前 manifests/changelogs 构造独立 publish plan；
- manifest dependency 与显式 `depends-on` 的传播语义不同；
- stable/named channel 是 package 发布状态，不是 changelog tag。

## 7. 信息架构

英文为无 locale 前缀的 canonical 内容，中文使用相同 slug 并加 `/zh`。

```text
/
/docs/
  introduction
  getting-started/
    installation
    first-release
    adopt-existing-monorepo
    github-actions
  workflow/
    changesets
    preview-release
    version
    release-pull-request
    publish
    recover-failures
  workspace/
    package-discovery
    config-sync
    dependencies
    rust
    nodejs
    python
    cpp
  versioning/
    bump-rules
    release-channels
    changelogs
  automation/
    github-actions
    workflow-outputs
    mcp
  plugins/
    overview
    quick-start
    protocol
    capabilities
    configuration
  configuration/
    overview
    packages
    resolvers
    publish-hooks
    pre-checks
    templates
  reference/
    cli
    configuration
    changeset-format
    template-contexts
    environment
  troubleshooting/
```

首页不进入 docs sidebar。它只负责：

- 用一句具体价值表达说明 Semifold；
- 展示 release workflow；
- 给出“第一次发布”和“理解工作流”两个主要入口；
- 展示按 discovery/version/dependency/publish 拆分的 ecosystem capability；
- 链接当前版本、GitHub 与双语入口。

## 8. 页面与写作规范

### 8.1 页面类型

- Tutorial：从可复现起点获得完整结果；
- How-to：解决一个明确任务；
- Explanation：解释领域规则和权衡；
- Reference：完整、精确、可扫描的公开契约。

一个页面只能有一个主要意图。不得把 tutorial 与完整 reference 混在同一页面。

### 8.2 页面结构

面向任务的页面依次包含：

1. 读完能得到什么；
2. 前置条件；
3. 最短默认路径；
4. 命令或配置；
5. 预期输出和产生的文件；
6. 常见失败与恢复；
7. 下一步。

默认路径先出现；操作系统、package manager 与 ecosystem 变体使用共享且持久化的 Tabs。代码块必须在
窄屏内部滚动，不得让页面产生水平滚动。

### 8.3 术语与 CLI

- 面向用户统一使用 `smif`；第一次出现时说明 `semifold` 是等价长名称；
- command、flag、配置 key、package ID 与路径保留原文；
- 中文不翻译 changeset、release plan、PackageId、ecosystem、resolver、adapter、plugin 等会影响检索的术语，
  首次出现时补充中文解释；
- 不使用“zero pain”“stable”等无法验证的绝对营销表述；
- 每项 ecosystem 能力分别说明 discovery、version edit、dependency propagation 与 publish，而不是一个总状态。

## 9. 内容事实来源

事实优先级从高到低为：

1. 已发布 tag 对应的公开 CLI 和配置行为；
2. 当前 PRD；
3. 已通过的领域/fixture/CLI 测试；
4. Rust 类型与公开 TypeScript SDK；
5. changeset 与 changelog；
6. 手写解释。

CLI reference 必须由 `clap` command surface 生成或由快照检查约束。配置示例必须能被当前 loader 解析；
关键工作流示例必须在 fixture 中运行。文档不得以 README、旧文档或 TODO 作为独立事实来源。

## 10. 技术架构

### 10.1 框架

`docs` package 使用：

- Next.js 16；
- Fumadocs Core、UI 与 MDX；
- React 19；
- Tailwind CSS 4；
- pnpm workspace；
- TypeScript strict mode。

不采用 Fumapress/Waku；当前需求需要对路由、静态导出、双语和首页进行明确控制。也不采用 Astro + React
islands，避免再引入一层 UI runtime 边界。

### 10.2 静态部署

Next.js 使用 `output: 'export'`，部署产物为 `docs/out`。GitHub Pages workflow 不运行 Node server。
静态导出必须验证：

- 根首页；
- 英文与中文 docs 首页；
- 至少一个三层深度页面；
- `/api/search` 静态 JSON；
- `llms.txt` 与 `llms-full.txt`；
- 404 页面。

是否启用 `trailingSlash` 由纵向切片通过 GitHub Pages 目录行为决定；验收要求旧 clean URL 和新 canonical
URL 都能直达，不以本地 Next dev server 行为代替验证。

### 10.3 国际化路由

- 英文保持 `/docs/...`；
- 中文保持 `/zh/docs/...`；
- 首页为 `/` 与 `/zh/`；
- 静态站不使用 cookie locale 或 middleware locale negotiation；
- 两个显式 route tree 共享同一 page renderer；
- Fumadocs i18n fallback 设为 `null`；
- 每个页面输出 canonical、`hreflang="en"`、`hreflang="zh-CN"` 与 `x-default`。

### 10.4 搜索

使用 Fumadocs built-in ZBSearch 静态模式：

- server route 使用 `staticGET`；
- UI 使用 `staticClient`；
- 查询按当前 locale 过滤；
- 中英文正文、标题与 heading 均进入索引；
- 搜索 index 大小进入构建报告，超过约定预算后再评估 cloud search。

### 10.5 LLM 文本

Fumadocs MDX 开启 processed Markdown 输出，并静态生成：

- `/llms.txt`：页面索引；
- `/llms-full.txt`：完整公开文档；
- 可选的逐页 Markdown route 在静态导出纵向切片验证后决定。

下一版本或未发布页面不得混入默认 `llms-full.txt`，除非文本中明确携带 availability。

## 11. 视觉与组件边界

- 使用 Fumadocs DocsLayout、DocsPage、search dialog 和默认可访问性行为；
- 只通过 design tokens、全局样式、首页与少量领域组件定制；
- 不复制或 fork Fumadocs sidebar、search、mobile nav 的内部实现；
- 保留现有 Semifold logo，但不使用 emoji 作为主要 feature icon；
- 正文目标宽度约 720–800px，reference table 可在容器内滚动；
- 提供浅色/深色主题，并尊重系统偏好；
- 交互目标、焦点可见性和颜色对比满足 WCAG 2.2 AA。

领域组件限制为具有明确复用价值的内容，例如 ReleaseFlow、EcosystemMatrix、ExpectedOutput 与
Availability。纯装饰效果不新增自定义 runtime 组件。

## 12. 迁移与 URL 策略

先建立旧 URL inventory。每个旧 URL 必须归入：

- 保留原内容语义；
- 合并到新页面；
- 拆分后跳转到主要 successor；
- 明确废弃。

GitHub Pages 不提供通用 HTTP 301。优先保留仍合理的 slug；无法保留时生成静态 redirect page，包含
canonical、可见链接和无脚本可用说明。不得只使用客户端 router redirect。

旧 Rspress 内容只作为事实线索，不逐文件搬迁。新页面完成并通过双语与事实检查后再删除对应旧页面。

## 13. 质量保障

新增 `docs:check`，至少执行：

- TypeScript 与 MDX 类型检查；
- Fumadocs type generation；
- Biome check；
- 内部 URL、heading anchor 与 MDX component link 检查；
- 英中页面、meta 与导航 parity；
- CLI reference drift；
- config/changeset 示例 fixture；
- Next static production build；
- 对 `out` 启动静态服务器后的 direct-route smoke test；
- 390px 与 1440px 的关键页面 layout smoke test；
- locale search smoke test。

CI 中 docs job 与 Rust tests 分离，避免文档依赖安装拖慢纯 Rust 反馈。部署 job 必须依赖 docs check 成功，
并上传 `docs/out` 而不是 Rspress `docs/doc_build`。

## 14. 实施阶段

### 阶段 0：事实审计与 PRD

- 确认 release/main/pending 边界；
- 记录公开 CLI、配置、plugin、MCP 和发布能力；
- 更新本 PRD 和 TODO。

### 阶段 1：纵向技术切片

- 替换 docs package 依赖与 scripts；
- 建立 Next/Fumadocs shell；
- 完成首页、Introduction、First Release 与 CLI reference 样例；
- 验证静态双语路由、搜索、LLM、GitHub Pages 与移动端。

### 阶段 2：核心成功路径

- 重写 installation、first release、adoption、changeset/status/version/publish/CI；
- 加入 expected output 与 failure recovery；
- 完成英文后同步中文，不允许长期 fallback。

### 阶段 3：Workspace、配置与自动化

- 补齐四个内置 ecosystem；
- 补齐 config sync、dependencies、channels、templates、pre-check 与 workflow outputs；
- 重写 MCP 为当前 CRUD surface。

### 阶段 4：Plugin 文档

- SDK 未发布前标记 next release；
- 文档只覆盖 runtime 已实现能力；
- Vite plugin 完成并发布后再补写 bundle 工作流。

### 阶段 5：迁移与切换

- 完成旧 URL map 和 redirect pages；
- 运行所有 docs checks；
- 更新部署 workflow；
- 删除 Rspress 专用配置与组件；
- 生产环境 smoke test。

## 15. 验收标准

- 不熟悉 Semifold 的用户不读源码即可完成初始化、创建 changeset，并解释 `smif status`；
- 所有生产导航内容对应已发布能力，下一版本内容具有明确 availability；
- 当前 CLI command/flag、配置 key、MCP tool 和 plugin protocol 不再引用旧 surface；
- 每个旧公共 URL 能访问等价内容或静态跳转；
- 英中页面与导航集合完全一致；
- 中英文搜索只返回当前 locale 的正确结果；
- 390px 页面无整体水平滚动，代码块和宽表格只在自身容器滚动；
- 所有深层页面可从静态服务器直接访问；
- `docs:check` 和 production build 在 CI 中通过；
- Lighthouse Accessibility 不低于 95，Performance 不低于 90；
- 生产部署不依赖 Node server、cookie locale 或运行时 rewrite。

## 16. 已确定决策

- 使用 Next.js + Fumadocs，不使用 Fumapress/Waku；
- 第一阶段继续使用 GitHub Pages 静态部署；
- 英文无前缀，中文使用 `/zh`；
- 使用静态 ZBSearch；
- 保留现有 logo，重做首页结构但不先做品牌重设计；
- 首页与用户文档描述已发布版本，下一版本功能显式标注；
- plugin runtime/SDK 与 Vite bundler 分开描述；
- 第一阶段不加入需要服务端的 AI chat 或 feedback backend。

## 17. 当前实施状态

截至 2026-08-10，阶段 0 与阶段 1 已完成：

- docs package 已迁移到 Next.js 16、Fumadocs Core/UI/MDX、React 19 与 Tailwind CSS 4；
- 英文 `/docs` 与中文 `/zh/docs` 使用两个显式静态 route tree；
- 首页、Introduction、First Release 与 CLI reference 样例已完成英中双语重写；
- 静态 ZBSearch、`llms.txt`、`llms-full.txt`、canonical 与 hreflang 已生成；
- `docs:check` 已覆盖源码生成、TypeScript、locale 文件 parity、静态构建、必需输出、静态内部链接与 locale search 配置；
- 本地静态服务器的根路由、三层深路由、中英文路由、search/LLM 输出与 404 smoke test 通过；
- 1440px 首页与 390px 深层教程完成真实 Chromium 截图检查。
- 本任务新增独立 `@semifold/docs` changeset 后，当前 release plan 为 16 个 changeset、6 个 package，文档 package 计划从 `1.1.0-beta.0` 提升到 `1.1.0-rc.0`。
- 旧 Rspress URL 已建立 inventory，并生成 canonical、可见链接和 HTML refresh 完整的静态 redirect page；Pages workflow 已切换为先运行 `docs:check`，再上传 `docs/out`。

阶段 2 至阶段 4 与阶段 5 的内容清理仍在进行。旧 Rspress 内容和配置暂时保留为事实核对与迁移线索；生产构建已经切换到 Fumadocs，但当前导航仍只包含纵向切片，不能被解释为完整内容重写已经结束。
