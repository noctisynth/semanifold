# Semifold 架构重构 TODO

> 详细设计见 [Rust 架构重设计方案](docs/prd/rust-architecture-redesign.md)。

## 总体目标

- [ ] 以跨生态 `WorkspaceGraph` 和不可变 `ReleasePlan` 取代以 `Resolver` 为中心的设计
- [ ] 以可配置 `ReleaseUnit` 支持单包、lockstep、多项目和 workspace 级发布流
- [ ] 让 `status`、`version`、`publish` 和 CI 使用同一套应用服务
- [ ] 将领域计算与文件系统、Git、HTTP、GitHub 和子进程副作用分离
- [x] 使用 `smif config sync` 增量同步 `.changes/config.toml`，不再依赖重复执行 `init`
- [ ] 保持现有 CLI 和配置格式的向后兼容

## 阶段 0：建立重构安全网

### Changeset 与版本规则

- [x] 为 changeset 解析、序列化和清理建立测试
- [x] 覆盖多个 changeset 的 bump level 合并
- [x] 覆盖 semantic major、minor、patch 计算
- [x] 覆盖当前 prerelease 初次生成、递增、切换 tag 和退出 prerelease 行为
- [x] 记录当前已知缺陷，避免将错误行为固化为新规范
- [x] 为通用 `ReleaseChannel::{Stable, Named}` 建立领域测试
- [x] 覆盖缺省 `channel` 与 `channel = "stable"` 的等价性
- [x] 覆盖首次进入命名通道的 stable 基准提升、通道内序号推进、通道切换和 `Unchanged` 不推进
- [x] 覆盖 changeset changelog tag 不影响版本通道
- [x] Rust 运行时内部依赖仅在计划新版本不满足 manifest 约束时传播 patch bump

### Ecosystem fixtures

- [x] 建立 Rust fixture：单包、workspace、内部依赖、workspace dependency、private package
- [x] 建立 Node.js fixture：单包、npm workspace、pnpm workspace、dependencies、peerDependencies
- [x] 建立 Python fixture：PEP 621、Poetry、Hatch、setup.cfg、常见 monorepo 目录
- [x] 建立 C++ fixture：CMakeLists.txt、vcpkg.json
- [x] C++ 支持根 CMake workspace 的直接 `add_subdirectory` 成员发现与 `target_link_libraries` 内部依赖排序
- [x] 为 manifest 解析结果建立 snapshot/golden tests
- [x] 为版本重写后的完整 manifest 建立 golden tests
- [x] 确保与版本无关的字段和格式不会被破坏

### 阶段完成条件

- [x] 四个生态均至少覆盖单包、工作区发现、内部依赖和版本重写
- [x] 现有 CLI 核心行为具备可回归的基线

## 阶段 1：引入 `semifold-core`

### Crate 与基础类型

- [x] 创建 `crates/semifold-core`
- [x] 定义 `PackageId` newtype
- [x] 定义 `Ecosystem`
- [x] 定义 `PackageSnapshot`
- [x] 定义 `Dependency` 和 `DependencyKind`
- [x] 定义 `ChangesetId`、`ReleaseReason` 和 `PlanWarning`
- [x] 确保 core 不依赖 `git2`、`reqwest`、`octocrab`、`clap` 或 `inquire`

### Workspace graph

- [x] 实现 `WorkspaceGraph`
- [x] 验证重复 `PackageId`
- [x] 验证未知内部依赖
- [x] 实现确定性拓扑排序
- [x] 对无依赖节点使用稳定排序
- [x] 检测依赖环并返回完整环路
- [x] 为多层依赖、菱形依赖、无关节点和依赖环建立单元测试

### Release planner

- [x] 定义不可变、可序列化的 `ReleasePlan`
- [x] 定义 `PackageRelease` 和完整 `VersionMap`
- [x] 实现纯 `ReleasePlanner`
- [x] 合并多个 changeset 的发布原因和 bump level
- [x] 在写文件前计算所有包的下一版本
- [x] 明确 stable / 命名版本通道与依赖传播规则
- [x] 临时将现有 resolver 结果转换为 `PackageSnapshot`

### 接入 `status`

- [x] 将 `smif status` 切换为渲染 `ReleasePlan`
- [x] 保持现有终端输出语义
- [x] CI PR comment 使用同一 `ReleasePlan`
- [x] 删除 `status` 内重复的版本计算

### 阶段完成条件

- [x] `status` 不再自行计算 bump
- [x] 相同输入始终生成相同 `ReleasePlan`
- [x] 依赖环错误包含可读的完整路径

## 阶段 2：实现 `smif config sync`

### CLI

- [x] 新增 `smif config migrate`
- [x] 新增 `smif config migrate --check`
- [x] 新增 `smif config channel set <channel> --package <PackageId>` 与 `--all`
- [x] 新增 `smif config channel clear --package <PackageId>` 与 `--all`
- [x] 支持 `smif config channel ... --check`
- [x] 新增 `smif config sync`
- [x] 新增 `smif config sync --check`
- [ ] 新增 `smif config sync --prune`
- [ ] 支持重复指定 `--resolver <type>`
- [x] 明确全局 `--dry-run` 与 `--check` 的不同退出语义
- [ ] JSON 配置返回 `UnsupportedConfigFormat`

### 同步计划

- [x] 定义 `ConfigSyncPlan`
- [x] 分类 added、missing、renamed、moved 和 conflicts
- [x] 实现 package path 规范化
- [x] 优先使用 resolver + path 匹配重命名
- [x] 使用 package identity 匹配路径移动
- [x] 多义匹配时停止写入并报告冲突
- [x] Resolver 变化时要求显式处理，不自动覆盖
- [x] Rename 时检查现有 changeset 对旧包名的引用并警告

### TOML 增量编辑

- [x] 将 legacy `version-mode` 增量迁移为 `channel`，并保留无关字段和注释
- [x] `channel` 与 `version-mode` 同时存在时停止并报告冲突
- [x] 实现 `TomlConfigEditor`
- [x] 使用 `toml_edit::DocumentMut` 读取和修改原始文档
- [x] 修改前将文档反序列化为强类型 `Config` 并验证
- [x] 只更新 `[packages]` 下需要变化的 table
- [x] 保留 `[release]`、`[[release.units]]` 和其他发布策略配置
- [x] 保留注释、空行、字段顺序和未知字段
- [x] 保留 `assets`、`channel` 和 `depends-on` 等手工字段
- [ ] 在配置模型中以 `ReleaseChannel::{Stable, Named}` 取代 `VersionMode`
- [ ] 读取时接受缺省 `channel` 和 `channel = "stable"`，并将二者解析为 stable
- [ ] 新增 stable package 时省略 `channel`，无关同步时保留用户显式的 `channel = "stable"`
- [ ] 将 `channel` 与 changeset / changelog 的 tag 完全分离
- [ ] 由各 ecosystem adapter 验证并编码命名通道版本；支持 post-release 生态格式
- [x] 新增 package 时只写入最小默认配置
- [ ] 新增条目采用确定性顺序，但不重排现有条目
- [x] 默认只报告缺失 package
- [ ] 仅在完整扫描成功且指定 `--prune` 时删除 package
- [x] 修改后再次反序列化并验证 `Config`
- [x] 使用临时文件和 rename 原子写回
- [x] 内容无变化时不写文件

### 与 `init` 复用

- [x] 抽取 resolver registry
- [x] 抽取统一 package discovery 服务
- [x] 抽取 package path 规范化
- [x] 抽取默认 `PackageConfig` 生成
- [x] 让 `init` 和 `config sync` 使用相同发现逻辑
- [ ] 将 `init --force` 从日常工作区同步路径中移除

### 测试

- [x] `config migrate --check` 检测到旧字段时返回非零且不写文件
- [x] 连续执行两次 `config migrate` 时第二次无 diff
- [x] channel set、clear 与 `--all` 保留目标 table 的其他字段，并且重复执行无 diff
- [x] channel `--check` 检测到状态不匹配时返回非零且不写文件
- [x] 新增 package 只产生最小 TOML diff
- [x] 缺失 package 默认保留
- [ ] `--prune` 显式删除缺失 package
- [ ] 扫描失败、部分 resolver 或匹配歧义时禁止 prune
- [x] Rename 和 move 保留原 table 的注释与自定义字段
- [x] `--check` 检测到漂移时返回非零退出码且不写文件
- [x] 连续执行两次同步时第二次无 diff

### 阶段完成条件

- [ ] 工作区增删包后无需重新执行 `init`
- [x] `config sync` 只产生最小、可审查的 TOML diff
- [x] CI 可以使用 `config sync --check` 检测配置漂移
- [x] 阶段完成前清除为分阶段接线临时添加的全部 `#[allow(dead_code)]`，并以无豁免的 Clippy 验证

## 阶段 3：统一版本修改的 Plan/Validate/Apply

### 文件修改模型

- [ ] 定义 `FileEdit`、`FileHash` 和 `EditSource`
- [ ] 在计划阶段生成所有 manifest 和 changelog 修改
- [ ] 检查多个 edit 是否修改同一文件
- [ ] 使用 expected hash 检测规划后的并发修改
- [ ] 实现临时文件写入和原子替换
- [ ] 所有文件成功写入后再删除 changeset
- [ ] 明确 post-version 失败后的恢复策略

### Rust 与 Node.js

- [ ] 将 Rust resolver 改为根据完整 `VersionMap` 生成 `FileEdit`
- [ ] 正确更新 Rust dependencies、dev-dependencies、build-dependencies 和 workspace dependencies
- [ ] 将 Node.js resolver 改为生成 `FileEdit`
- [ ] 正确更新 dependencies、devDependencies、peerDependencies 和 optionalDependencies
- [ ] 移除 Rust/Node.js 对 `Context.version_bumps` 的依赖
- [ ] Resolver 不再直接写文件或处理 dry-run

### Changelog

- [ ] 分离 Git/Forge 元数据收集与 Markdown 格式化
- [ ] 定义不可变 `ChangelogContext`
- [ ] 让 changelog 格式化成为纯函数
- [x] 为依赖传播自动加入发布闭包的 package 生成 `Dependencies` changelog 条目
- [ ] 将 changelog 修改表示为 `FileEdit`
- [ ] 明确 GitHub 元数据查询失败时的降级策略

### 接入 `version`

- [ ] `smif version` 消费与 `status` 相同的 `ReleasePlan`
- [ ] `smif version --dry-run` 只渲染计划，不调用写入器或命令运行器
- [ ] 删除 `version` 内逐包填充可变版本 map 的逻辑
- [ ] 返回结构化 `ApplyReport`

### 阶段完成条件

- [ ] `status`、`version --dry-run` 与 `version` 使用相同计划
- [ ] 任一验证失败时工作区保持不变
- [ ] 文件修改不再依赖包处理顺序

## 阶段 4：将 Resolver 收敛为 Ecosystem Adapter

### 新接口

- [ ] 定义 `EcosystemAdapter`
- [ ] 实现 `discover()`
- [ ] 实现 `inspect()`
- [ ] 实现 `plan_edits()`
- [ ] Adapter 接收完整 `VersionMap`

### 迁移

- [ ] 迁移 Rust adapter
- [ ] 迁移 Node.js adapter
- [ ] 迁移 Python adapter
- [ ] 迁移 C++ adapter
- [ ] 删除 `Resolver::sort_packages()`
- [ ] 删除 `Resolver::publish()`
- [ ] 删除 adapter 对 `Context` 的依赖
- [ ] 删除 adapter 内的 dry-run 分支
- [ ] 将 `semifold-resolver` 收敛或重命名为 `semifold-ecosystems`

### 跨生态依赖

- [ ] 在配置中增加可选 `depends-on`
- [ ] 将 manifest 内部依赖与显式跨生态依赖合并到 `WorkspaceGraph`
- [ ] 定义相同包名跨生态冲突策略
- [ ] 定义 dev、peer、optional 和 build dependency 的排序与传播策略

### 阶段完成条件

- [ ] Adapter 只负责发现、解析和变更规划
- [ ] 所有包顺序由统一 `WorkspaceGraph` 计算
- [ ] 跨生态依赖参与同一拓扑排序

## 阶段 5：统一发布引擎

### Release unit 与发布身份

- [ ] 定义 `ReleaseUnit`、`ReleaseIdentityStrategy` 和 `ResolvedReleaseUnit`
- [ ] 为 `Package`、`SharedVersion`、`Static` 和 `Fingerprint` identity 建立解析与校验规则
- [ ] 单包仓库自动形成 package identity release unit
- [ ] 多包仓库未配置 release unit 时保留固定 `release` 分支
- [ ] 为单包、入口包、lockstep、静态分支和多 release unit 建立规划测试

### Publish plan

- [ ] 定义 `PublishPlan` 和 `PackagePublish`
- [ ] 依据 `ReleasePlan` 解析每个 `ReleaseUnit`
- [ ] 为 package、shared-version、static 和 fingerprint identity 生成 `ResolvedReleaseUnit`
- [ ] 将 release branch / release PR 规划归属到 `ReleaseUnit`
- [ ] 明确 package-level Git tag 与 release-unit branch identity 的区别
- [ ] 将 preflight、commands 和 assets 纳入计划
- [ ] 基于 `WorkspaceGraph` 生成确定性发布顺序
- [ ] 为 private package 和已发布版本提供显式 skip reason

### 外部能力

- [ ] 抽取 `CommandRunner`
- [ ] 抽取 `RegistryClient`
- [ ] 抽取 `ForgeClient`
- [ ] 将 GitHub release 创建移到 Forge adapter
- [ ] 将 asset upload 移到 Forge adapter
- [ ] 删除四个生态中重复的 publish 命令执行代码

### 执行与报告

- [ ] 在执行发布前完成所有可执行的 preflight
- [ ] 定义 `ReleaseContext`、`PackageContext` 和只读 `TemplateContext`
- [ ] 分支模板只暴露 `release.*`
- [ ] changelog、GitHub Release 与包级命令模板暴露 `package.*`
- [ ] 使用 MiniJinja 严格未定义变量模式
- [ ] 在渲染后校验 branch ref、Git tag 和命令参数
- [ ] 模板中引用不适用的 `release.tag` 或 `release.version` 时返回配置错误
- [ ] 支持 dry-run 发布计划
- [ ] 返回结构化 `PublishReport`
- [ ] 报告 succeeded、skipped、failed 和 not-started package
- [ ] 明确部分发布失败后的退出码和恢复指引

### 阶段完成条件

- [ ] Ecosystem adapter 不再执行任何外部命令
- [ ] `publish` 与 CI 使用相同 `PublishPlan` 和 publisher

## 阶段 6：拆分 `Context` 并收敛入口层

### 项目加载

- [ ] 定义字段完整的 `Project`
- [ ] 为 `init` 定义独立 `ProjectLocation`
- [ ] 定义结构化 `ProjectLoadError`
- [ ] 不再使用 `.ok()` 吞掉配置、路径或 Git 错误
- [ ] GitHub 环境、Git 句柄和 dry-run 不进入 `Project`
- [ ] 将发布事实建模为不可变 `ReleaseContext`，而不是恢复万能 `Context`
- [ ] 将包级事实建模为 `PackageContext`
- [ ] 将 changelog 所需事实建模为 `ChangelogContext`
- [ ] 确保 context 不持有 Git、HTTP、文件系统、resolver 或可变缓存

### 应用服务

- [ ] 创建 `semifold-engine`
- [ ] 实现 `SemifoldService::plan_config_sync()`
- [ ] 实现 `SemifoldService::apply_config_sync()`
- [ ] 实现 `SemifoldService::plan_release()`
- [ ] 实现 `SemifoldService::apply_release()`
- [ ] 实现 `SemifoldService::plan_publish()`
- [ ] 实现 `SemifoldService::publish()`

### CLI、CI 与 MCP

- [ ] CLI 只负责参数解析、交互和结果渲染
- [ ] CI 通过 `SemifoldService` 编排 release branch、commit、push 和 PR
- [ ] MCP 使用与 CLI 相同的应用服务
- [ ] MCP 不再为每次调用重新构建全局 `Context`
- [ ] MCP 不再使用 `set_current_dir()` 修改进程全局状态
- [ ] 移除旧 `Context.version_bumps`
- [ ] 移除旧 `Context::create_resolver()`
- [ ] 删除旧 `Context`

### 错误边界

- [ ] Core 使用结构化 `DomainError`
- [ ] Ecosystem adapter 使用结构化 `AdapterError`
- [ ] Engine 使用结构化 `AppError`
- [ ] 仅在 CLI 最外层使用 `anyhow` 补充上下文
- [ ] 本地化和终端着色不进入 core 或 engine

### 阶段完成条件

- [ ] CLI 模块中不再包含版本计算或 manifest 文件操作
- [ ] CLI、CI 和 MCP 不复制业务编排
- [ ] 发布计算中不存在 `RefCell` 或隐式全局可变 map

## 最终验收

- [ ] `cargo fmt --all --check` 通过
- [ ] `cargo clippy --workspace --all-targets --all-features` 通过
- [ ] `cargo test --workspace --all-features` 通过
- [ ] `status` 与 `version` 的计划结果完全一致
- [ ] 跨生态依赖图支持确定性拓扑排序和环检测
- [ ] 所有文件修改在写入前完成计划和验证
- [ ] `--dry-run` 不产生文件、命令、网络或发布副作用
- [x] `config sync` 保留 TOML 注释、顺序、未知字段和手工配置
- [x] `config sync --check` 可稳定用于 CI
- [ ] 连续执行配置同步和版本规划均具有幂等性
- [ ] release branch / release PR 支持单包、lockstep、静态和多单元发布策略
- [ ] 模板在严格模式下渲染，且不会为多包发布隐式选择 package tag
- [ ] 现有 CLI 主要用法和配置文件保持兼容

## 实施前需要确认的决策

- [ ] 内部依赖变化触发 patch bump 的条件
- [ ] dev、peer、optional 和 build dependency 的传播规则
- [ ] 相同包名跨生态时的 `PackageId` namespace
- [ ] Post-version 命令失败后的文件恢复策略
- [ ] GitHub 元数据查询失败时是否降级
- [ ] 是否长期支持 JSON 配置更新
- [ ] `config sync` 遇到未启用 resolver 时的行为
- [ ] 是否提供 `--rewrite-changesets` 辅助包重命名
- [ ] 一个 package 是否允许同时属于多个 `ReleaseUnit`
- [ ] `Fingerprint` identity 的格式与适用场景

## 低优先级优化

- [ ] 定义 `PackageReleaseState`，区分首次发布与已有发布历史
- [ ] 将 registry package metadata 查询与目标版本 `version_exists` pre-check 分离
- [ ] 确定首次发布状态查询失败、离线模式和无 registry 配置时的行为
