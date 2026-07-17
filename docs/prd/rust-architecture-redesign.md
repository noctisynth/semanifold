# Semifold Rust 架构重设计方案

## 1. 文档状态

- 状态：Draft
- 范围：Semifold Rust workspace 的领域模型、crate 边界、生态适配器、版本规划和发布执行流程
- 不包含：文档站、官网视觉、品牌和营销内容
- 原则：优先替换内部模型，第一阶段不改变 CLI 用法和现有配置格式

### 1.1 文档与变更治理

本 PRD 是 Semifold Rust 架构的技术事实来源。任何需求、架构或设计变更在实现前必须先更新本 PRD，并在 PRD 与目标实现一致后才可继续。

根目录 `TODO.md` 仅记录本 PRD 与当前代码之间尚未完成的差异：

- 当 PRD 新增了尚未在代码中实现的行为时，必须在 `TODO.md` 新增或更新对应待办；
- 当代码完成某项 PRD 定义的行为并通过适当验证时，必须更新 `TODO.md`，移除或勾选对应待办；
- `TODO.md` 不是独立需求来源；需求以 PRD 为准。

`Cargo.toml`、workspace members、依赖和 crate 元数据不得被手工编辑。所有此类变更必须经由 Cargo CLI 完成，例如 `cargo add`、`cargo remove`、`cargo new` 或 `cargo init`。如果 Cargo CLI 无法表达所需变更、命令失败，或其结果与 PRD 不一致，必须停止并向用户确认，不得回退为手动编辑 `Cargo.toml`。

## 2. 背景

Semifold 的核心定位是管理跨语言 monorepo 中的 changeset、版本、内部依赖、changelog 和包发布。现有实现已经支持 Rust、Node.js、Python 和 C++，但核心设计仍然以单个 `Resolver` 和 CLI 命令流程为中心，没有建立统一的跨生态工作区模型。

这导致不同命令重复计算发布状态，依赖顺序由各 resolver 局部处理，版本修改依赖运行期可变状态，并且领域计算与文件、Git、HTTP 和子进程副作用直接耦合。

## 3. 现状诊断

### 3.1 `Context` 承担过多职责

当前 `Context` 同时保存：

- Semifold 配置；
- changeset 路径与配置路径；
- 仓库根目录；
- GitHub 仓库信息；
- `git2::Repository`；
- resolver 工厂；
- dry-run 状态；
- 版本计算过程中的 `version_bumps` 可变缓存。

`Context::create()` 中多次使用 `.ok()` 将发现或打开错误转换为 `None`。配置不存在、Git 仓库无法打开和路径解析失败因此很难区分。

`RefCell<HashMap<String, Version>>` 使版本修改依赖包处理顺序。Resolver 只能看到此前已经写入缓存的新版本，无法在修改文件前检查完整结果。

当前 `VersionMode::PreRelease` 还将发布通道直接绑定到 SemVer prerelease 字符串：它会在没有 changeset 时推进序号，并以 resolver 的 SemVer 表达作为所有生态的共同规则。这个行为不应成为新设计的兼容目标；新的 `ReleaseChannel` 必须以 changeset 为驱动，并由各生态 adapter 负责版本编码与校验。

### 3.2 `Resolver` 的职责边界错误

当前 `Resolver` trait 同时负责：

1. 发现包；
2. 解析 manifest；
3. 修改 manifest 和版本文件；
4. 排序所有包；
5. 执行发布命令。

其中发布命令来自用户配置，并不是生态解析器的能力。Rust、Node.js、Python 和 C++ resolver 的 `publish()` 实现因此几乎完全重复。

Resolver 还直接依赖 `Context`，导致其无法作为纯粹的生态适配器独立测试。

### 3.3 依赖顺序不是真正的拓扑排序

现有 Rust、Node.js 和 Python resolver 使用 `Vec::sort_by()` 比较两个包是否直接依赖。该 comparator 不具备完整依赖图信息，不能可靠处理传递依赖，也可能不满足排序比较器要求的传递性。

例如：

```text
A → B → C
```

两两比较不能稳定保证得到 `C, B, A`。

每个 resolver 还只处理相同生态的依赖。项目中不存在一张统一的跨语言依赖图，因此无法系统性处理 Rust 包与 Node.js 绑定包等跨生态关系。

### 3.4 `status`、`version` 和 `publish` 重复推导状态

三个命令分别执行包解析、bump 计算和依赖处理：

- `status` 独立计算当前版本与下一版本；
- `version` 再次计算，并立即修改文件；
- `publish` 重新发现和排序包。

这三条路径没有共享同一个发布计划，因此难以保证预览、版本修改和发布顺序始终一致。

### 3.5 领域计算与副作用混合

当前 `version` 流程在单个循环内交错执行：

```text
解析包
→ 计算新版本
→ 写入 manifest
→ 更新 Context.version_bumps
→ 查询 Git/GitHub 元数据
→ 生成并写入 changelog
→ 处理下一个包
```

任一中间步骤失败都可能留下部分修改的工作区。`dry_run` 判断也因此散落在 CLI 和各 resolver 内部。

### 3.6 测试基础不足

当前 Rust 源码中基本没有针对以下核心行为的自动化测试：

- changeset 解析和合并；
- 版本通道与版本序号规则；
- 依赖传播和拓扑顺序；
- 各生态 manifest 的解析与重写；
- dry-run 与真实执行的一致性；
- 发布前检查和部分失败。

在缺少特征测试的情况下直接替换 resolver 实现，风险过高。

## 4. 设计目标

### 4.1 主要目标

1. 建立统一的跨生态 `WorkspaceGraph`。
2. 使 `status` 和 `version` 消费同一个不可变 `ReleasePlan`。
3. 将计算与文件、Git、HTTP、GitHub 和命令执行副作用分离。
4. 使生态适配器可以通过 fixture 独立测试。
5. 使 dry-run 成为执行器策略，而非遍布代码的条件分支。
6. 为依赖环、重复包名、manifest 不一致和文件修改冲突提供明确错误。
7. 保持现有 CLI 和配置向后兼容，允许渐进迁移。

### 4.2 非目标

- 不在第一阶段重新设计 CLI。
- 不立即替换 TOML/JSON 配置格式。
- 不为每个小模块创建独立 crate。
- 不在第一阶段引入动态插件或 WASM resolver。
- 不尝试回滚已经完成的外部 registry 发布。

## 5. 核心设计：Plan → Validate → Apply

Semifold 应当在任何写文件或执行外部命令前，先构建完整、不可变、可序列化的计划。

```text
Config + Changesets + Ecosystem manifests
                    │
                    ▼
              WorkspaceSnapshot
                    │
                    ▼
               WorkspaceGraph
                    │
                    ▼
                ReleasePlan
                    │
          ┌─────────┼─────────┐
          ▼         ▼         ▼
       render    validate    apply
      (status)              (version)
```

`publish` 基于当前 `WorkspaceSnapshot` 和配置构建 `PublishPlan`，然后进行 registry preflight 与顺序执行。

## 6. 领域模型

### 6.1 基础类型

```rust
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageId(String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ecosystem {
    Rust,
    Node,
    Python,
    Cpp,
}

pub struct PackageSnapshot {
    pub id: PackageId,
    pub manifest_name: String,
    pub version: semver::Version,
    pub ecosystem: Ecosystem,
    pub path: Utf8PathBuf,
    pub publishable: bool,
    pub dependencies: Vec<Dependency>,
}

pub struct Dependency {
    pub package: PackageId,
    pub kind: DependencyKind,
    pub requirement: Option<String>,
}
```

`PackageId` 是 Semifold 配置和依赖图中的稳定身份，不应继续在所有层使用无约束 `String`。加载时应验证配置键、manifest 包名和依赖引用的关系。

### 6.2 `WorkspaceGraph`

```rust
pub struct WorkspaceGraph {
    packages: BTreeMap<PackageId, PackageSnapshot>,
    dependencies: DependencyGraph,
}
```

`WorkspaceGraph` 负责：

- 检测重复 `PackageId`；
- 解析内部依赖边；
- 合并 manifest 依赖和配置补充的跨生态依赖；
- 检测依赖环并返回完整环路；
- 生成确定性拓扑顺序；
- 为版本传播和发布顺序提供统一输入。

拓扑排序由 core 一次完成，不再由各生态 adapter 修改同一个 `Vec`。对于无依赖关系的节点，使用 `PackageId` 排序保证输出稳定。

### 6.3 `ReleasePlan`

```rust
pub struct ReleasePlan {
    pub packages: Vec<PackageRelease>,
    pub order: Vec<PackageId>,
    pub consumed_changesets: Vec<ChangesetId>,
    pub warnings: Vec<PlanWarning>,
    pub file_edits: Vec<FileEdit>,
}

pub struct PackageRelease {
    pub id: PackageId,
    pub ecosystem: Ecosystem,
    pub current_version: semver::Version,
    pub next_version: semver::Version,
    pub bump: BumpLevel,
    pub reasons: Vec<ReleaseReason>,
}
```

`ReleasePlan` 构建完成后必须包含所有包的新版本。生态 adapter 因此可以一次获得完整 `VersionMap`，不再需要 `Context.version_bumps`。

changeset 只能在 manifest、changelog 和 post-version 命令均成功后删除。post-version 失败时必须保留 changeset；后续阶段再将整个流程收敛为原子 `Plan → Validate → Apply`。

`status` 只渲染该计划，`version` 验证并应用该计划。

### 6.4 `FileEdit`

```rust
pub struct FileEdit {
    pub path: Utf8PathBuf,
    pub expected_hash: FileHash,
    pub new_content: String,
    pub source: EditSource,
}
```

Adapter 生成内容修改，但不直接写文件。`expected_hash` 用于确保从规划到执行期间目标文件没有被其他进程改动。

执行前还应检查多个修改是否同时针对同一文件。同一文件的多个变更必须先合并或报告冲突，不允许依赖执行顺序相互覆盖。

### 6.5 `PublishPlan`

```rust
pub struct PublishPlan {
    pub packages: Vec<PackagePublish>,
}

pub struct PackagePublish {
    pub id: PackageId,
    pub version: semver::Version,
    pub preflight: Option<RegistryCheck>,
    pub commands: Vec<CommandSpec>,
    pub assets: Vec<ReleaseAsset>,
}
```

Registry pre-check、发布命令和 GitHub release 不应存在于 ecosystem adapter 中。它们由 engine 根据统一包模型和用户配置组装。

### 6.6 `ReleaseUnit`、发布身份与模板上下文

一个发布分支或 release PR 不必等同于一个 package。Semifold 应将其建模为 `ReleaseUnit`：一次发布流程的命名、成员和分支边界。

```rust
pub struct ReleaseUnit {
    pub name: ReleaseUnitName,
    pub members: Vec<PackageId>,
    pub identity: ReleaseIdentityStrategy,
    pub branch_template: Template,
}

pub enum ReleaseIdentityStrategy {
    /// 以一个成员包的下一版本定义发布身份。
    Package { package: PackageId },
    /// 要求所有成员的下一版本相同。
    SharedVersion,
    /// 使用固定字符串；适合长期更新的稳定 release 分支。
    Static { value: String },
    /// 使用本次计划的稳定摘要；仅适合高级自动化。
    Fingerprint,
}
```

`Package { package }` 是“入口项目”场景的一个 identity 策略，而不是所有仓库必须设置的 `primary-package`。例如，`semifold` 可以用 CLI package 的下一版本为 release PR 命名，同时在同一个 release unit 内升级 resolver 和 changelog crate。

```toml
[[release.units]]
name = "cli"
members = ["semifold", "semifold-resolver", "semifold-changelog"]
identity = { kind = "package", package = "semifold" }
branch = "release/{{ release.tag }}"
```

一个 lockstep 组可以使用：

```toml
[[release.units]]
name = "sdk"
members = ["sdk-core", "sdk-node", "sdk-python"]
identity = { kind = "shared-version" }
branch = "release/v{{ release.version }}"
```

一个没有单一版本身份的 workspace 可以使用稳定分支：

```toml
[[release.units]]
name = "workspace"
members = ["*"]
identity = { kind = "static", value = "workspace" }
branch = "release/{{ release.identity.value }}"
```

#### 模板变量作用域

模板渲染必须在 `ReleasePlan` 完成后执行。分支、changelog、GitHub Release 和发布命令不共享一个无约束 map，而使用按场景构造的不可变数据视图：

```rust
pub struct ReleaseContext {
    pub project: ProjectInfo,
    pub unit: ResolvedReleaseUnit,
    pub plan: ReleasePlanSummary,
    pub repository: Option<RepositoryInfo>,
    pub ci: Option<CiInfo>,
}

pub struct PackageContext {
    pub release: ReleaseContext,
    pub package: PackageRelease,
}

pub struct ChangelogContext {
    pub package: PackageContext,
    pub changeset: ChangesetView,
    pub commit: Option<CommitInfo>,
    pub pull_request: Option<PullRequestInfo>,
}
```

最终传入 MiniJinja 的 `TemplateContext` 只序列化上述事实，不携带 Git client、文件系统、HTTP client、resolver 或可变缓存。

- 分支模板只暴露 `release.*`。它代表一个 release unit，不能隐式挑选多个 package 中的某一个。
- 包级发布命令、GitHub Release 和 changelog 模板暴露 `package.*`，每个 package 有自己的 `package.tag` 和 `package.next_version`。
- `release.tag` 与 `release.version` 仅在 identity 能够唯一提供它们时存在；例如 `Package` 和 `SharedVersion`。
- 使用 `Static` 或 `Fingerprint` identity 时，引用未定义的 `release.tag` 必须报错。

MiniJinja 必须使用严格未定义变量模式，并在渲染后验证 branch ref、git tag 和命令参数的目标格式。当前 `Config.tags` 是 changelog 分类，不得作为 `release.tag` 的来源。

#### 默认行为

- 单包仓库：自动形成一个 package identity release unit，默认分支模板为 `release/{{ release.tag }}`。
- 多包仓库且没有 release unit：保持现有固定 `release` 分支，避免猜测某个包是“主项目”。
- 多包仓库有入口项目：用户显式配置 `Package` identity。
- 需要多个独立发布流的仓库：定义多个 `ReleaseUnit`；一个 package 同时属于多个 unit 时必须显式处理冲突。

## 7. 生态适配器

### 7.1 接口

```rust
pub trait EcosystemAdapter: Send + Sync {
    fn ecosystem(&self) -> Ecosystem;

    fn discover(
        &self,
        root: &Utf8Path,
    ) -> Result<Vec<PackageSnapshot>, AdapterError>;

    fn inspect(
        &self,
        package: &PackageLocation,
    ) -> Result<PackageSnapshot, AdapterError>;

    fn plan_edits(
        &self,
        package: &PackageSnapshot,
        versions: &VersionMap,
    ) -> Result<Vec<FileEdit>, AdapterError>;
}
```

### 7.2 职责边界

Adapter 可以：

- 读取并解析 Cargo.toml、package.json、pyproject.toml 等文件；
- 发现生态内包；
- 提取包名、版本、发布属性和内部依赖；
- 根据完整 `VersionMap` 生成新文件内容。

Adapter 不可以：

- 持有或读取全局 `Context`；
- 决定全局包顺序；
- 直接写文件；
- 执行子进程；
- 访问 registry 或 GitHub；
- 处理 dry-run。

### 7.3 跨生态依赖

某些跨生态依赖无法从 manifest 自动推导，例如 Node.js 包使用本地 Rust binding 产物。因此配置需要提供可选显式边：

```toml
[packages.node-binding]
path = "packages/node"
resolver = "nodejs"
depends-on = ["rust-core"]
```

该字段在新实现稳定后作为向后兼容的可选配置引入，不阻塞第一阶段迁移。

## 8. Crate 和模块边界

建议使用四层结构，避免过度拆分：

```text
crates/
├── semifold-core/
│   ├── package.rs
│   ├── dependency.rs
│   ├── changeset.rs
│   ├── workspace.rs
│   ├── release_plan.rs
│   └── versioning.rs
│
├── semifold-ecosystems/
│   ├── rust.rs
│   ├── node.rs
│   ├── python.rs
│   └── cpp.rs
│
├── semifold-engine/
│   ├── loader.rs
│   ├── config_sync.rs
│   ├── config_editor.rs
│   ├── planner.rs
│   ├── executor.rs
│   ├── publisher.rs
│   └── ports.rs
│
└── semifold/
    ├── cli/
    ├── github/
    ├── mcp/
    └── main.rs
```

### 8.1 `semifold-core`

纯领域层，不依赖：

- `git2`；
- `reqwest`；
- `octocrab`；
- `clap`；
- `inquire`；
- 真实文件系统和子进程。

可以依赖 `semver`、`serde`、`thiserror` 和轻量路径类型。

### 8.2 `semifold-ecosystems`

包含各生态 manifest 解析和内容重写。它依赖 core，但 core 不依赖它。

现有 `semifold-resolver` 建议渐进演化为这一层，而不是一次性删除后重写。

### 8.3 `semifold-engine`

应用层负责：

- 加载项目；
- 组合生态 adapter；
- 构建 `WorkspaceGraph`；
- 构建 `ReleasePlan` 和 `PublishPlan`；
- 验证并执行文件修改；
- 通过 port 调用 Git、HTTP、命令和 Forge 实现。

### 8.4 `semifold`

仅负责入口和表现：

- Clap 参数解析；
- 交互提示；
- 本地化文案；
- 日志与终端输出；
- GitHub Actions 编排；
- MCP transport 和工具参数映射。

CLI 命令不再直接读写 manifest 或实现版本计算。

## 9. 项目加载与 `Context` 拆分

对于要求项目已初始化的命令，使用完整类型：

```rust
pub struct Project {
    pub root: Utf8PathBuf,
    pub changeset_dir: Utf8PathBuf,
    pub config_path: Utf8PathBuf,
    pub config: Config,
}
```

`Project::load()` 不应将错误吞掉为 `None`：

```rust
pub enum ProjectLoadError {
    RepositoryNotFound,
    ChangesetDirectoryNotFound,
    ConfigNotFound,
    ConfigInvalid { path: Utf8PathBuf, source: ConfigError },
    RepositoryOpenFailed { path: Utf8PathBuf, source: GitError },
}
```

`init` 使用独立的发现结果：

```rust
pub struct ProjectLocation {
    pub root: Utf8PathBuf,
    pub existing_config: Option<Utf8PathBuf>,
}
```

GitHub 环境、Git 仓库和 dry-run 均不是 `Project` 数据的一部分。

项目级稳定数据、一次 release 的动态事实，以及包或 changelog 的模板数据应分层表示为 `Project`、`ReleaseContext`、`PackageContext` 和 `ChangelogContext`。这些 context 可以为文本生成提供丰富信息，但必须是不可变数据快照；Git、Forge、文件系统和 resolver 等能力通过 engine 的依赖注入提供，而不放回 context。

## 10. 副作用边界

应用层通过小型 port 表达需要的外部能力：

```rust
pub trait FileSystem {
    fn read(&self, path: &Utf8Path) -> Result<Vec<u8>, FsError>;
    fn write_atomic(&self, path: &Utf8Path, data: &[u8]) -> Result<(), FsError>;
    fn remove(&self, path: &Utf8Path) -> Result<(), FsError>;
}

pub trait CommandRunner {
    fn run(&self, command: &CommandSpec) -> Result<CommandOutput, CommandError>;
}

#[async_trait]
pub trait RegistryClient {
    async fn version_exists(&self, check: &RegistryCheck) -> Result<bool, RegistryError>;
}

pub trait GitRepository {
    fn is_clean(&self) -> Result<bool, GitError>;
    fn commit_for_path(&self, path: &Utf8Path) -> Result<Option<CommitInfo>, GitError>;
}

#[async_trait]
pub trait ForgeClient {
    async fn pull_request_for_commit(
        &self,
        commit: &CommitId,
    ) -> Result<Option<PullRequestInfo>, ForgeError>;
}
```

不需要在第一次迁移中抽象所有 IO。优先抽出 `FileSystem`、`CommandRunner` 和 `RegistryClient`，因为它们直接决定 plan/apply 的可测试性。

## 11. 文件修改与失败策略

`version` 执行流程应为：

1. 加载并规划所有变更。
2. 验证依赖图、新版本和文件修改冲突。
3. 根据 `expected_hash` 确认文件未在规划后改变。
4. 将新内容写入临时文件。
5. 以尽可能原子的方式替换目标文件。
6. 写入 changelog。
7. 成功后删除 changeset。
8. 最后执行非事务性 post-version 命令。

Post-version 子进程不可能真正纳入跨进程事务。在执行前应尽可能完成所有静态验证，并在失败时清楚报告已完成和未完成步骤。

`--dry-run` 只执行规划和验证并渲染结果，不调用写入器或命令运行器。

## 12. Changelog 设计

Changelog 生成应分为两部分：

1. 可选元数据收集：从 Git 和 Forge 获取 commit、PR 和 author；
2. 纯格式化：根据 changeset、版本和元数据生成 Markdown。

格式化器接收 `ChangelogContext` 或其序列化后的模板视图，不应持有 `Context`、`git2::Repository` 或自行创建 `Octocrab`。GitHub 查询失败是否中断版本流程也应是显式策略，而不是格式化函数的隐式行为。

依赖传播而自动加入发布闭包的 package 仍会被发布，不能生成空 changelog。规划器必须为它记录 `ReleaseReason::DependencyPropagation { dependency, next_version }`；格式化器将该原因渲染为独立的 `Dependencies` 区段，例如：

```markdown
### Dependencies

- Update semifold-resolver to 0.4.0-alpha.0.
```

该条目不伪装成用户 changeset，也不使用 changeset 的 changelog tag。只有实际会发布的 dependent 才生成该记录；dev、build 或其他不影响发布产物的依赖传播策略由 adapter 规则单独定义。

## 13. `config` 指令设计

### 13.1 目标

`init` 只负责首次创建 Semifold 配置。工作区在后续开发中新增、删除、移动或重命名包时，不应要求用户重复执行 `init`，也不应覆盖已经手工维护的发布命令、assets、version mode 和跨生态依赖。

新增配置同步入口：

```text
smif config sync
```

该命令重新扫描已配置的生态系统，将实际工作区与 `.changes/config.toml` 中的 `[packages]` 进行比较，并使用 `toml_edit` 对原始 TOML 文档进行局部修改。

核心要求：

- 保留注释、字段顺序、空行和用户自定义字段；
- 只修改需要同步的 package table；
- 默认不静默删除配置；
- 支持只检查、不写入；
- 同一输入重复执行结果不变；
- `init` 和 `config sync` 复用同一套包发现服务。

### 13.2 CLI 语义

第一版建议提供：

```text
smif config sync
smif config sync --check
smif config sync --prune
smif config sync --resolver rust --resolver nodejs
```

参数语义：

| 参数 | 行为 |
| --- | --- |
| 无参数 | 扫描配置中已经启用的 resolver，展示差异并应用安全更新 |
| `--check` | 只检查工作区与配置是否一致；存在差异时返回非零退出码，适合 CI |
| `--prune` | 删除已经无法发现的 package 配置；默认只报告，不删除 |
| `--resolver <type>` | 只同步指定 resolver；可以重复传入 |

全局 `--dry-run` 继续有效，但语义与 `--check` 不同：

- `--dry-run` 输出将要应用的完整 `ConfigPlan`，但不写文件，退出码不表示配置漂移；
- `--check` 用于断言配置已经同步，发现任何需要修改的内容时返回非零退出码。

第一版只更新 `.changes/config.toml` 或 `.changesets/config.toml`。如果当前项目使用 JSON 配置，命令返回明确的 `UnsupportedConfigFormat`，避免 JSON 重写造成无关格式变化。未来如需支持 JSON，应单独定义格式保留策略。

### 13.2.1 旧配置迁移

为使仓库能够从旧的 `version-mode` 过渡到 `channel`，提供独立且不执行 workspace discovery 的入口：

```text
smif config migrate
smif config migrate --check
```

迁移只处理 `[packages.*]` 中的版本通道字段，并使用 `toml_edit::DocumentMut` 保留其余内容：

- `version-mode = "semantic"` 或缺省语义模式迁移为缺省 `channel`（删除旧字段，不写入 `channel = "stable"`）；
- `version-mode = { pre-release = { tag = "alpha" } }` 迁移为 `channel = "alpha"`；
- 已使用 `channel` 的 package 保持不变；同一 package 同时设置 `channel` 与 `version-mode` 时停止并报告冲突；
- JSON 配置返回明确的不支持错误；
- `--check` 在存在可迁移条目时返回非零且不写文件；全局 `--dry-run` 只报告将要迁移的条目，退出成功；
- 成功迁移后再次运行不得产生 diff。

该命令是格式迁移工具，不替代 `config sync`，也不自动将 stable package 显式改写为 `channel = "stable"`。

### 13.2.2 发布通道管理

为显式管理已配置 package 的发布通道，提供以下命令：

```text
smif config channel set alpha --package semifold
smif config channel set alpha --package semifold --package semifold-resolver
smif config channel set alpha --all
smif config channel clear --package semifold
```

`set` 只接受非空的命名通道；`stable` 是保留值，恢复 stable 必须使用 `clear`。命令必须指定一个或多个 `--package <PackageId>`，或显式指定 `--all`，两者不能同时使用。未知 package 是错误，避免因拼写失误产生无效配置。

`set` 仅修改目标 package 的 `channel` 字段。`clear` 删除该字段，使 package 回到缺省 stable 状态。二者都使用 `toml_edit::DocumentMut` 与原子写回，保留目标 table 的其他字段、注释和所有非目标 package。无实际变化时不得写入文件。

全局 `--dry-run` 只输出将修改的 package 而不写入；`--check` 断言目标已处于请求状态，存在需要修改的 package 时返回非零。JSON 配置不受支持。

### 13.3 同步计划

与 release 流程相同，配置更新也采用 Plan/Validate/Apply：

```rust
pub struct ConfigSyncPlan {
    pub config_path: Utf8PathBuf,
    pub added: Vec<DiscoveredPackage>,
    pub removed: Vec<ConfiguredPackage>,
    pub renamed: Vec<PackageRename>,
    pub moved: Vec<PackageMove>,
    pub conflicts: Vec<ConfigConflict>,
}
```

执行流程：

```text
读取原始 config.toml
→ 解析 Config 与 toml_edit::DocumentMut
→ 使用 ecosystem adapters 重新发现包
→ 计算 ConfigSyncPlan
→ 检查冲突和删除策略
→ 对 DocumentMut 做局部修改
→ 原子写回 config.toml
```

领域层只计算 `ConfigSyncPlan`。`toml_edit::DocumentMut` 属于配置文件 adapter，不进入 `semifold-core`。

### 13.4 包匹配规则

同步不能只按包名匹配，否则重命名会被误判为“一删一增”，并丢失原配置中的手工字段。建议按以下优先级匹配：

1. resolver 与规范化 package path 完全相同：视为同一个包，manifest name 改变时识别为 rename；
2. `PackageId` 或 manifest name 相同：path 改变时识别为 move；
3. 名称和路径均不匹配：视为新增和缺失；
4. 多个包同时命中同一候选：产生冲突，不自动修改。

规范化路径必须：

- 相对于项目根目录；
- 使用统一分隔符；
- 移除 `.` 和可安全消解的 `..`；
- 不跟随项目根目录外的符号链接。

Rename 时应尽量移动原有 TOML table，而不是重新创建，以保留：

- `channel`；
- `assets`；
- `depends-on`；
- 未来新增的未知字段；
- table 前后的注释和装饰信息。

如果存在尚未消费且引用旧包名的 changeset，rename 必须产生警告。第一版不自动重写 changeset，避免修改用户提交的变更记录。

### 13.5 新增、删除和更新策略

#### 新增包

新发现的包加入 `[packages]`：

```toml
[packages.example]
path = "packages/example"
resolver = "rust"
```

稳定通道为默认值时不显式写入 `channel`。`assets`、`depends-on` 等无法从 manifest 推导的字段不自动生成。

新条目采用确定性顺序插入。建议在 `[packages]` 内按 `PackageId` 排序新增条目，但不重排现有条目，避免产生大面积无意义 diff。

### 13.6 版本通道

包的发布状态由可选的 `channel` 表达，而不是由 `VersionMode` 或 `pre-release` 概念表达：

```toml
[packages.semifold]
path = "crates/semifold"
resolver = "rust"
channel = "alpha"
```

`channel` 的规则如下：

- 缺省 `channel` 与 `channel = "stable"` 等价，均表示稳定通道；
- `stable` 是保留关键字，不能作为自定义通道；
- 任何其他非空值都是命名发布通道，例如 `alpha`、`beta`、`next`、`nightly` 或 `internal.2026`；
- `[tags]` 和 changeset 中的 tag 仅用于 changelog 分类，绝不决定版本通道或版本号；
- 包从 stable 首次进入某个命名通道时，changeset 的最高 `major`、`minor` 或 `patch` 决定该通道周期对应的下一稳定基准版本；该基准版本不需要额外的手工设定。

领域层使用下列抽象：

```rust
pub enum ReleaseChannel {
    Stable,
    Named(String),
}
```

配置加载时，缺省值和 `stable` 都解析为 `ReleaseChannel::Stable`；其余值解析为 `ReleaseChannel::Named`。配置同步对新 stable package 省略 `channel`，但不应因无关同步操作删除用户显式写入的 `channel = "stable"`。

`ReleaseChannel` 是发布流程概念，不是 SemVer 的 `Prerelease` 概念。核心仅处理通道状态与序号推进，具体版本字符串由 ecosystem adapter 按各自版本规范编码和验证：

| 生态 | `channel = "alpha"` 的一种编码 | `channel = "post"` 的一种编码 |
| --- | --- | --- |
| Rust / Node（SemVer） | `1.0.0-alpha.1` | `1.0.0-post.1` |
| Python（PEP 440） | `1.0.0a1` 或项目约定格式 | `1.0.0.post1` |

因此，`channel` 的字符串不在 core 中按 SemVer 限制；adapter 必须在规划阶段验证该通道能否表示为对应生态的合法版本，并在无法表示时返回结构化错误。

版本通道的状态完全由当前版本和当前 `channel` 决定，不能依赖已经被 version 命令消费的 changeset。规则如下：

- stable 包首次进入命名通道时，先按 changeset 计算下一稳定基准，再生成 `<base>-<channel>.0`；例如 `0.2.16 + major + alpha` 生成 `1.0.0-alpha.0`；
- 当前包已处于相同命名通道时，任意非 `Unchanged` changeset 都只推进通道序号；例如 `1.0.0-alpha.0 + major` 生成 `1.0.0-alpha.1`。这代表同一个待发布稳定版本周期内的后续变更，不重复提升 stable 基准；
- 切换到另一个命名通道时保留稳定基准并将序号重置为 `.0`；例如 `1.0.0-alpha.2 → beta` 生成 `1.0.0-beta.0`；
- `Unchanged` 不生成新版本；
- 从命名通道回到 stable 时移除 prerelease 后缀并发布当前基准，例如 `1.0.0-alpha.2 → 1.0.0`。

stable 包的 major/minor/patch 计算保持现有语义。上述规则不能从 changelog tag 推断，adapter 仍负责将领域通道编码为所属生态的合法版本。

#### 删除包

默认行为只报告：

```text
- package old-core is configured but no longer exists
  run 'smif config sync --prune' to remove it
```

只有 `--prune` 才删除 table。以下情况即使指定 `--prune` 也必须拒绝自动删除：

- 多个 resolver 扫描发生错误，无法确认包确实已删除；
- 包路径暂时不可访问；
- 存在匹配歧义；
- 本次使用 `--resolver` 排除了该包所属 resolver。

#### 移动和重命名

路径移动只更新 `path`。重命名更新 package table key，并保留原 table 内容。若新名称已经存在，产生冲突并停止写入。

#### Resolver 变化

同一路径从一种生态变成另一种生态时，不应自动覆盖 `resolver`。这通常意味着项目结构发生重大变化或发现器判断错误，必须作为冲突要求用户确认。

### 13.7 使用 `toml_edit` 保格式更新

当前 `save_config()` 将强类型 `Config` 整体重新序列化，会丢失用户原有格式选择和部分注释。`config sync` 不应调用该路径。

建议增加独立组件：

```rust
pub struct TomlConfigEditor {
    path: Utf8PathBuf,
    document: toml_edit::DocumentMut,
}

impl TomlConfigEditor {
    pub fn load(path: &Utf8Path) -> Result<Self, ConfigEditError>;
    pub fn validate(&self) -> Result<Config, ConfigEditError>;
    pub fn apply(&mut self, plan: &ConfigSyncPlan) -> Result<(), ConfigEditError>;
    pub fn render(&self) -> String;
}
```

编辑时直接操作：

```rust
let packages = document["packages"]
    .as_table_mut()
    .ok_or(ConfigEditError::MissingPackagesTable)?;
```

实现要求：

- 加载后先反序列化成强类型 `Config` 完成语义验证；
- 使用 `DocumentMut` 修改原文档，而不是从 `Config` 重新序列化；
- 只修改 `[packages]` 下需要同步的 table；`[release]` 与 `[[release.units]]` 等发布策略配置完全保留；
- 保留未知字段，确保旧版 Semifold 不会抹掉新版或插件写入的配置；
- 写回前再次从修改后的文档反序列化并验证；
- 使用临时文件与 rename 原子替换；
- 文件内容未变化时不执行写入。

### 13.8 与 `init` 的关系

`init` 和 `config sync` 应共享：

- resolver registry；
- package discovery；
- package path 规范化；
- 默认 package 配置生成；
- 冲突诊断。

区别仅在于：

- `init` 从空配置生成初始文档和 CI 模板；单包仓库可生成 package identity 的默认 release unit，多包仓库默认保留固定 `release` 分支；
- `config sync` 从现有 `DocumentMut` 生成并应用增量修改；
- `init --force` 也不应继续成为日常同步工作区的方式。

长期可将 `init` 的配置生成实现为“创建最小文档后应用一次完整 `ConfigSyncPlan`”，避免两套包发现和配置生成逻辑再次分叉。

### 13.9 输出示例

```text
Configuration drift detected:

  + crates/new-adapter       rust
  ~ packages/node            moved from bindings/node
  → old-python               renamed to python-core
  - crates/legacy            missing (kept; use --prune to remove)

Updated .changes/config.toml
```

无变化时：

```text
Configuration is up to date.
```

输出结构应来源于 `ConfigSyncPlan`，终端渲染、JSON 输出和未来 MCP 工具可以复用同一个结果。

## 14. CLI、CI 与 MCP

### 14.1 CLI

CLI 仅负责：

1. 解析参数；
2. 调用 application service；
3. 渲染结果和错误。

```rust
pub struct SemifoldService<D> {
    deps: D,
}

impl<D: Dependencies> SemifoldService<D> {
    pub fn plan_config_sync(
        &self,
        project: &Project,
        options: ConfigSyncOptions,
    ) -> Result<ConfigSyncPlan, AppError>;
    pub fn apply_config_sync(
        &self,
        plan: ConfigSyncPlan,
    ) -> Result<ConfigSyncReport, AppError>;
    pub fn plan_release(&self, project: &Project) -> Result<ReleasePlan, AppError>;
    pub fn apply_release(&self, plan: ReleasePlan) -> Result<ApplyReport, AppError>;
    pub async fn plan_publish(&self, project: &Project) -> Result<PublishPlan, AppError>;
    pub async fn publish(&self, plan: PublishPlan) -> Result<PublishReport, AppError>;
}
```

### 14.2 CI

CI 编排仍然可以处理 release branch、commit、push 和 Pull Request，但必须调用同一 `SemifoldService`，不再直接复用 CLI 模块中的具体实现函数。

### 14.3 MCP

MCP 服务不应在每个工具调用中重新构建全局 `Context`，也不应依赖 `set_current_dir()` 修改进程全局状态。

MCP handler 应持有已加载的 service 或显式 `ProjectLocator`，然后调用与 CLI 相同的 changeset 和规划接口。

## 15. 错误模型

`anyhow` 适合 CLI 最外层补充上下文，但 core、ecosystems 和 engine 内应返回分层错误：

```text
DomainError
├── DuplicatePackage
├── UnknownDependency
├── DependencyCycle
└── InvalidVersionTransition

AdapterError
├── ManifestNotFound
├── ManifestParse
├── UnsupportedManifestShape
└── EditConflict

AppError
├── ProjectLoad
├── Domain
├── Adapter
├── FileSystem
├── Git
├── Registry
└── Command
```

CLI 负责将这些错误转换成本地化用户消息，不应让翻译宏进入 core。

## 16. 测试策略

### 16.1 领域单元测试

- changeset 的 bump level 合并；
- stable 与命名通道的版本计算；
- 依赖传播；
- 确定性拓扑排序；
- 依赖环诊断；
- 同一组输入产生完全相同的 `ReleasePlan`。

### 16.2 Ecosystem fixture/golden tests

每个生态建立 `fixtures/`：

```text
fixtures/rust/
├── single-package/
├── workspace/
├── workspace-dependencies/
├── optional-dependencies/
└── unpublished-package/
```

测试内容：

- 发现的 `PackageSnapshot`；
- 解析的内部依赖；
- 给定 `VersionMap` 后产生的 `FileEdit`；
- 应用 edit 后的完整 golden manifest；
- 原始文件中与版本无关的格式和字段不被破坏。

### 16.3 Executor 测试

- hash 冲突时拒绝写入；
- 任一预写入失败时不删除 changeset；
- dry-run 没有任何写操作；
- 计划与最终文件完全一致；
- post-version 命令的顺序和失败报告。

### 16.4 CLI 端到端测试

- `smif status`、`smif version --dry-run` 与实际 `smif version` 展示相同计划；
- 未初始化项目返回稳定错误码；
- dirty Git 工作区策略；
- 发布 preflight 跳过已存在版本；
- CI 流程调用同一 release plan。

### 16.5 Config sync 测试

- 新增包只增加一个 table，且第二次执行无 diff；
- 默认不删除缺失包，`--prune` 明确删除；
- 扫描失败时 `--prune` 不产生删除；
- rename 和 move 保留 assets、version mode、未知字段与注释；
- 匹配歧义和 resolver 变化产生冲突；
- `--check` 在漂移时返回非零退出码且不写文件；
- 修改前后均可反序列化成有效 `Config`；
- 已有 package table 不因为新增包而被整体重排；
- JSON 配置返回明确的不支持错误；
- 修改后的文档再次同步是幂等的。
- `config migrate` 将 legacy `version-mode` 转换为 `channel`，保留无关字段与注释；
- `config migrate --check` 在存在迁移项时不写文件并返回非零；
- 同时存在 `channel` 与 `version-mode` 时迁移拒绝写入。
- `config channel set` 与 `clear` 仅修改指定 package 的 `channel` 字段，并保留 table 的其他内容；
- `config channel --check` 在目标 channel 不匹配时不写入并返回非零；
- `config channel --all` 显式应用至每个已配置 package，重复执行无 diff。

## 17. 迁移计划

### 阶段 0：特征测试

目标：锁定现有对外行为，为内部替换提供安全网。

- 为四个 resolver 建立基础 fixture；
- 覆盖现有 changeset 和 version bump 行为；
- 记录已知缺陷，避免将 bug 无意固化为新设计。

完成条件：各生态至少具有单包、monorepo 和内部依赖用例。

### 阶段 1：引入 core 与 `ReleasePlan`

目标：建立新领域模型，但暂时复用现有 resolver 获取包数据。

- 创建 `semifold-core`；
- 引入 `PackageId`、`WorkspaceGraph` 和 `ReleasePlan`；
- 实现真正的拓扑排序；
- 将 `status` 改为渲染 `ReleasePlan`。

完成条件：`status` 不再自行计算 bump，依赖环能够给出明确路径。

### 阶段 2：引入 `config sync`

目标：将首次初始化与后续工作区同步分离，并验证生态发现接口和 TOML 增量编辑边界。

- 引入 `ConfigSyncPlan`；
- 实现 `TomlConfigEditor`；
- 让 `init` 和 `config sync` 复用 package discovery；
- 实现 added、missing、rename、move 和 conflict 分类；
- 实现 `--check` 与 `--prune`；
- 为注释保留、未知字段保留和幂等性建立 golden tests。

完成条件：工作区新增或删除包后无需重新执行 `init`，`config sync` 只产生最小 TOML diff。

### 阶段 3：计划化文件变更

目标：让 `version` 消费与 `status` 相同的 plan。

- 引入 `FileEdit` 和 `VersionMap`；
- 改造 Rust 和 Node.js resolver，使其返回修改内容而不直接写入；
- 实现文件修改验证与统一应用；
- 删除 Rust/Node.js 对 `Context.version_bumps` 的依赖；
- 将 changelog 写入纳入同一执行过程。

完成条件：`status`、`version --dry-run` 和 `version` 使用相同计划。

### 阶段 4：完成 ecosystem adapter 迁移

目标：移除 resolver 的全局职责。

- 迁移 Python 和 C++；
- 删除 `Resolver::sort_packages()`；
- 删除 `Resolver::publish()`；
- 删除 adapter 对 `Context` 和 dry-run 的依赖；
- 将现有 `semifold-resolver` 收敛为 `semifold-ecosystems`。

完成条件：所有 adapter 仅执行发现、解析和变更规划。

### 阶段 5：发布引擎与外部边界

目标：统一 preflight、发布命令和 GitHub release。

- 引入 `PublishPlan`；
- 抽出 `CommandRunner` 和 `RegistryClient`；
- 将重复 publish 实现替换为统一 publisher；
- 将 GitHub release 和 asset upload 移到 Forge adapter；
- 为部分发布失败提供结构化 report。

完成条件：生态 adapter 不再运行任何外部命令。

### 阶段 6：拆分 `Context` 并收敛入口层

目标：清除全局可变状态和重复编排。

- 引入 `Project` 和 `ProjectLocation`；
- 移除 `Context.version_bumps`；
- 移除 `Context::create_resolver()`；
- CLI、CI 和 MCP 改用 `SemifoldService`；
- MCP 不再修改全局 current directory。

完成条件：旧 `Context` 删除，CLI 模块中不再出现领域计算或 manifest 文件操作。

## 18. 验收标准

1. `status` 和 `version` 基于同一 `ReleasePlan`。
2. 任何 ecosystem adapter 不依赖 `Context`、GitHub、HTTP 或子进程。
3. 包顺序由统一跨生态依赖图拓扑计算。
4. 依赖环返回包含完整环路的错误。
5. 所有文件修改在写入前已完整计划和验证。
6. `--dry-run` 不调用写入器、命令运行器或发布客户端。
7. 各生态至少有单包、workspace、内部依赖和版本重写 fixture。
8. CLI、CI 和 MCP 使用同一 application service，不复制发布计算。
9. 无任何发布计算依赖 `RefCell` 或处理顺序中逐步填充的全局 map。
10. 保持现有 CLI 主要用法和配置文件兼容，新的跨生态依赖配置为可选扩展。
11. `smif config sync` 能增量同步工作区包，并保留 TOML 注释、顺序、未知字段和手工配置。
12. 缺失包默认不删除，只有完整扫描成功且显式指定 `--prune` 时才允许删除。
13. `smif config sync --check` 可用于 CI 检测配置漂移。
14. 对同一工作区连续执行两次同步，第二次不产生文件修改。
15. release branch、release PR 与模板变量由 `ReleaseUnit` 决定，不依赖隐式主项目。
16. MiniJinja 模板严格校验未定义变量和渲染结果；多包 release unit 不会隐式选择某个 `package.tag`。

## 19. 开放决策

实施前还需要确定：

1. [已决定] 内部依赖包未在 changeset 中时，自动触发 patch bump；显式 changeset 的更高 bump 优先。
2. dev dependency 是否影响发布拓扑顺序和版本传播。
3. peer、optional 和 build dependency 分别采用什么传播策略。
4. 不同生态包名相同时，`PackageId` 是否需要 `ecosystem:name` namespace。
5. post-version 命令失败后，是自动恢复已修改文件，还是保留工作区并输出结构化恢复指引。
6. GitHub PR 元数据查询失败时，是否默认降级为无 PR 信息的 changelog，而不中断 `version`。
7. `config sync` 是否需要在后续版本支持 JSON 配置，还是正式将可编辑配置限定为 TOML。
8. 未启用 resolver 但发现对应生态 manifest 时，是提示用户启用，还是允许 `--resolver` 自动创建默认 resolver 配置。
9. rename 后是否提供独立 `--rewrite-changesets` 选项更新尚未消费的 changeset，默认行为仍是不修改。
10. 一个 package 是否允许属于多个 `ReleaseUnit`；若允许，如何界定它们的发布和版本修改冲突。
11. `Fingerprint` identity 的稳定输入、可见格式和适用场景。

## 20. 推荐的第一个实施切片

第一个可合并切片不应是大规模移动文件，而应是：

1. 新建 `semifold-core`；
2. 实现 `PackageId`、`PackageSnapshot`、`WorkspaceGraph` 和确定性拓扑排序；
3. 从现有 resolver 结果临时转换为 `PackageSnapshot`；
4. 实现纯 `ReleasePlanner`；
5. 仅将 `smif status` 切换到新 plan；
6. 为多层依赖、无关节点稳定顺序和依赖环建立测试。

该切片能够验证新架构的核心价值，同时不触碰真实文件写入和发布流程，回归风险最低。

## 21. 总结

Semifold 的中心抽象不应继续是同时解析、排序、写文件和发布的 `Resolver`。

新架构应当以：

- 跨生态 `WorkspaceGraph`；
- 不可变 `ReleasePlan`；
- 无副作用 `EcosystemAdapter`；
- 统一 Plan/Validate/Apply 执行模型

为核心。

这不只是为了代码整洁度，而是为了使“跨语言 monorepo 发布”成为实际存在于核心模型中的能力。
