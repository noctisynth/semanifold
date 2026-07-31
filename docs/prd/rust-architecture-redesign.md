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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
    pub source: DependencySource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyKind {
    Unspecified,
    Runtime,
    Development,
    Build,
    Optional,
    Peer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencySource {
    Manifest,
    Config,
}
```

`PackageId` 是 Semifold 配置和依赖图中的稳定身份，不应继续在所有层使用无约束 `String`。加载时应验证配置键、manifest 包名和依赖引用的关系。

`PackageId` 在一份 Semifold 配置中全局唯一，但不自动采用 `ecosystem:name` namespace。manifest name
只要求在同一生态内唯一：不同生态可以声明相同 manifest name，并通过用户选择的稳定 `PackageId`
（例如 `rust-shared` 与 `node-shared`）区分；同一生态内出现重复 manifest name 时无法无歧义绑定
manifest 依赖，工作区加载必须失败。manifest 依赖始终按 `(Ecosystem, manifest_name)` 解析，绝不因
跨生态名称相同而推断跨生态边；跨生态关系只能通过 `depends-on` 引用稳定 `PackageId`。

`DependencySource` 区分生态 manifest 推导的依赖与 `depends-on` 配置补充的依赖。两者都参与同一个
`WorkspaceGraph`；同一 package 同时通过两种来源指向同一目标时，图边去重，但配置来源的发布传播语义仍必须保留。

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

首个切片以 `WorkspaceGraph::new(Vec<PackageSnapshot>)` 接收已发现的 package。构建时仅将指向图内 package 的依赖作为内部依赖边；引用未知 package 时返回包含依赖方与被引用 `PackageId` 的领域错误。重复 `PackageId` 同样返回领域错误。`topological_order()` 返回“依赖在前、依赖方在后”的稳定顺序；若存在环，错误携带首尾相连的完整 `PackageId` 环路，便于后续 CLI 渲染诊断。

### 6.3 `ReleasePlan`

```rust
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChangesetId(String);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BumpLevel {
    Unchanged,
    Patch,
    Minor,
    Major,
}

pub type VersionMap = BTreeMap<PackageId, semver::Version>;

pub struct ReleasePlan {
    packages: Vec<PackageRelease>,
    versions: VersionMap,
    order: Vec<PackageId>,
    consumed_changesets: Vec<ChangesetId>,
    warnings: Vec<PlanWarning>,
    file_edits: Vec<FileEdit>,
}

pub struct PackageRelease {
    pub id: PackageId,
    pub ecosystem: Ecosystem,
    pub current_version: semver::Version,
    pub next_version: semver::Version,
    pub bump: BumpLevel,
    pub reasons: Vec<ReleaseReason>,
}

pub enum ReleaseReason {
    Changeset { changeset: ChangesetId },
    DependencyPropagation {
        dependency: PackageId,
        next_version: semver::Version,
    },
}

pub enum PlanWarning {
    NonPatchBumpOnPrerelease {
        package: PackageId,
        requested: BumpLevel,
    },
}
```

`ReleasePlan.packages` 只包含本次实际发布的 package，`versions` 则必须包含工作区所有 package 的计划后版本，未发布 package 使用当前版本。生态 adapter 因此可以一次获得完整 `VersionMap`，不再需要 `Context.version_bumps`。

`ReleasePlan` 只通过验证构造函数创建并提供只读访问器：每个发布 package 必须在 `versions` 中存在且版本等于 `next_version`；`order` 必须无重复地包含所有且仅包含发布 package；`consumed_changesets` 不得重复。违反这些条件返回领域错误。集合顺序由 planner 按 `PackageId`、`ChangesetId` 和 `WorkspaceGraph` 拓扑顺序确定，使序列化结果稳定。同一 changeset 或依赖传播原因在一个 package 上不得重复记录。

纯 planner 接收已解析的领域输入，不读取 changeset 文件、config 或 manifest：

```rust
pub struct ChangesetInput {
    pub id: ChangesetId,
    pub releases: BTreeMap<PackageId, BumpLevel>,
}

pub struct PackageReleasePolicy {
    pub channel: ReleaseChannel,
    pub propagating_dependencies:
        BTreeMap<PackageId, Option<semver::VersionReq>>,
}

pub struct ReleasePlanner;

impl ReleasePlanner {
    pub fn plan(
        graph: &WorkspaceGraph,
        changesets: &[ChangesetInput],
        policies: &BTreeMap<PackageId, PackageReleasePolicy>,
    ) -> Result<ReleasePlan, ReleasePlannerError>;
}
```

`propagating_dependencies` 是 adapter/engine 根据生态规则筛选并解析后的传播策略，而不是 manifest
所有依赖的副本。依赖是否参与排序与是否触发发布传播是两项独立决策：

- 所有已解析为内部边的 manifest `Runtime`、`Development`、`Build`、`Optional`、`Peer` 依赖都参与
  `WorkspaceGraph` 拓扑排序，确保版本修改、构建、测试与发布命令始终在依赖之后执行；
- 首版只有 Rust `[dependencies]` 的 `Runtime` 边进入约束感知的自动传播；
- manifest `Development`、`Build`、`Optional` 与 `Peer` 边不自动传播；Node.js、Python 与 C++ 的
  manifest `Runtime` 边也不自动传播，因为 npm、PEP 440 与 CMake 约束不能交给 Rust semver
  解析器近似判断；
- `source = Config` 的 `depends-on` 边不受类别限制，始终以无约束策略传播 patch，作为需要重新构建
  或重新发布时的显式选择。

`Some(requirement)` 在依赖的新版本仍满足约束时不传播；`None` 表示没有可验证的发布约束，依赖发生
发布时传播 patch。未来若为其他生态启用 manifest 自动传播，必须先由对应 adapter 将原生约束转换为
可验证的领域策略，不得在 application/core 中用 `semver::VersionReq` 猜测 npm 或 PEP 440 语义。

planner 合并同一 package 的最高 bump，并为每个贡献 changeset 保留独立原因。依赖约束失效时，将尚未发布的依赖方加入 patch 发布闭包；依赖方已有显式发布时保留其更高 bump，同时追加依赖传播原因。完整闭包计算后一次生成所有 package 的 `VersionMap`，再按 `WorkspaceGraph` 拓扑顺序生成发布顺序。

changeset 只能在 manifest、changelog 和 post-version 命令均成功后删除。post-version 失败时保留已经写入的文件和全部 changeset，不尝试跨进程回滚；`ApplyReport` 必须列出已替换文件、失败命令和仍待消费的 changeset，并提供恢复所需的事实。

`status` 只渲染该计划，`version` 验证并应用该计划。

在完整 `PublishPlan` 落地前，现有 `publish` 迁移层也必须使用同一个
`WorkspaceGraph.topological_order()` 生成 package 执行顺序；不得继续依次调用各
resolver 的局部 `sort_packages()`，因为直接依赖比较无法形成多层依赖的全局拓扑序，
会导致依赖包尚未发布时先发布依赖方。完整发布引擎仍在阶段 5 接管 preflight、skip
reason、命令和报告，但迁移期发布顺序必须立即满足“依赖先于依赖方”。

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

`FileHash` 必须是规划时读取到的原始文件字节的 SHA-256 摘要，并以小写十六进制编码。领域层通过 `FileHash::from_bytes(&[u8])` 生成该值；不接受由调用方任意构造的字符串。执行器在写入任何临时文件前读取每个目标文件并比较摘要，任一不匹配即返回冲突且不得写入任何目标文件。

执行前还应检查多个修改是否同时针对同一文件。同一文件的多个变更必须先合并或报告冲突，不允许依赖执行顺序相互覆盖。

第一版由应用层的 `FileEditExecutor` 接收项目根目录与 edit 列表。它只接受项目根目录内的相对路径，拒绝绝对路径和包含 `..` 的路径；先验证全部路径、重复目标和 `expected_hash`，随后把所有新内容写入同目录临时文件，最后逐个 `rename` 替换目标文件。任一前置验证或临时文件写入失败时不得替换任何目标文件，并应尽力清理已创建的临时文件；替换阶段发生 I/O 失败时返回已替换与未替换文件的结构化报告，恢复策略留待后续 `ApplyReport` 统一定义。

只读目标验证必须暴露为独立于 `FileEditExecutor` 的入口，并由 executor 与
`version --dry-run` 复用；dry-run 不得构造临时文件、调用命令运行器或收集远程
changelog 元数据。验证列表中任一较晚目标失败时，先前目标也不得被替换；临时文件准备
阶段失败时同样不得开始替换，并应清理已准备的临时文件。最终文件内容由已验证且目标唯一
的完整 edit 集合决定，不依赖 edit 或 package 的输入顺序。

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

在完整 `PublishPlan` 落地前，阶段 4 使用 application 层的统一发布命令执行桥接：按 package 的
拓扑顺序依次执行全部 `prepublish`，成功后再执行全部 `publish`；命令工作目录为 package path，任一
命令失败立即停止该 package 和后续发布。全局 dry-run 时，仅执行配置中
`dry_run = true` 的命令，其余命令明确报告跳过。private package、registry pre-check、GitHub Release
与 assets 仍由同一 application 流程编排，不回到 adapter。四个旧 resolver 不再暴露 `publish()`，
也不接收 dry-run；该桥接后续由 `PublishPlan` 与注入的 `CommandRunner` 取代。

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
pub struct PackageLocation {
    pub id: PackageId,
    pub project_root: Utf8PathBuf,
    pub path: Utf8PathBuf,
}

pub struct ManifestDependency {
    pub manifest_name: String,
    pub kind: DependencyKind,
    pub requirement: Option<String>,
}

pub struct PackageInspection {
    pub id: PackageId,
    pub manifest_name: String,
    pub version: semver::Version,
    pub ecosystem: Ecosystem,
    pub path: Utf8PathBuf,
    pub publishable: bool,
    pub dependencies: Vec<ManifestDependency>,
}

pub struct EcosystemPlanInput<'a> {
    pub project_root: &'a Utf8Path,
    pub workspace_packages: &'a [PackageSnapshot],
    pub released_packages: &'a [PackageId],
    pub versions: &'a VersionMap,
}

pub trait EcosystemAdapter: Send + Sync {
    fn ecosystem(&self) -> Ecosystem;

    fn discover(
        &self,
        root: &Utf8Path,
    ) -> Result<Vec<PackageInspection>, AdapterError>;

    fn inspect(
        &self,
        package: &PackageLocation,
    ) -> Result<PackageInspection, AdapterError>;

    fn plan_edits(
        &self,
        input: EcosystemPlanInput<'_>,
    ) -> Result<Vec<FileEdit>, AdapterError>;
}
```

`PackageLocation.project_root` 是已规范化的绝对项目根，`path` 是项目根内的规范化相对
package path。adapter 从 manifest 只能解析依赖声明中的原生名称，因此 `discover()` 与
`inspect()` 返回 `ManifestDependency`，不得在 adapter 内猜测该名称对应的 Semifold
`PackageId`。应用层收集完整的 `PackageInspection` 后，按 `(Ecosystem, manifest_name)`
建立到稳定 `PackageId` 的唯一映射，仅将同一生态内唯一匹配的声明转换为
`PackageSnapshot.dependencies`；未匹配项视为外部依赖，重复 manifest name 则在构造
`WorkspaceGraph` 前报错。这样配置键可以与 manifest 名称不同，adapter 也无需读取全局
配置。

`EcosystemPlanInput.workspace_packages` 只包含当前 adapter 所属生态的完整
package 快照，`released_packages` 只包含本次实际发布的对应生态 package，并且
`versions` 仍是全工作区完整版本映射。批量输入允许 Rust adapter 在一个调用中合并共享
workspace manifest；不拥有共享 manifest 的生态也必须返回与 released package 输入顺序
无关的稳定 edit 集合。

### 7.2 职责边界

Adapter 可以：

- 读取并解析 Cargo.toml、package.json、pyproject.toml 等文件；
- 发现生态内包；
- 提取包名、版本、发布属性和内部依赖；
- 根据完整 `VersionMap` 生成新文件内容。

每个生态 adapter 只拥有本生态的原生 manifest：Rust 拥有 `Cargo.toml`，Python 拥有
`pyproject.toml`、`setup.cfg` 和 Python 源码版本文件，Node 拥有 `package.json`。
Python 或 Node binding 可以读取 Rust manifest 作为动态版本来源或构建输入，但不得写入
`Cargo.toml`。跨生态 package 默认保持独立版本序列；Rust 产物变化是否触发 binding
重新发布由显式依赖传播决定，不能通过复制 Rust 版本隐式实现。需要同版本发布时使用
`ReleaseUnit::SharedVersion`；动态派生版本关系后续以显式 `version-source` 模型表达。

Rust manifest 与 Semifold TOML 配置必须使用保格式编辑器，保留无关的文本布局。`package.json` 则允许在完成 `serde_json::Value` 语义校验后使用启用 `preserve_order` 的序列化器规范化输出：对象键顺序必须保持，输出使用标准缩进并始终以一个换行结束。不得为 `package.json` 版本修改维护自定义字节级 JSON 解析或扫描器；JSON 结构、转义和边缘语法应完全由 `serde_json` 处理。

Rust adapter 规划依赖版本时必须同时处理 package manifest 中的 `dependencies`、
`dev-dependencies`、`build-dependencies`，以及 workspace 根 manifest 中的
`workspace.dependencies`。依赖使用别名时，以 `package` 字段解析真实 manifest name；
使用 `workspace = true` 时只修改共享的 `workspace.dependencies` 声明，不在成员
manifest 中插入重复版本。共享 workspace manifest 必须在一次 Rust 批量规划中合并所有
package version 与 dependency version 变化，同一路径只生成一个 `FileEdit`，结果不得依赖
release package 的遍历顺序。只有目标 package 的计划版本相对当前版本发生变化时才重写
对应依赖约束，避免将未变化的宽松约束无意义地规范化为精确版本。虚拟 workspace 根
没有可归属的 package 时，edit source 使用 `EditSource::WorkspaceDependencies` 并按
`PackageId` 稳定记录所有被更新的内部依赖。

Node adapter 解析 `package.json` 时，缺失 `version` 必须视为 `0.0.0`，以支持未声明版本的模板项目；显式但无效的 `version` 仍必须报告解析错误。版本写入和 `FileEdit` 规划必须在缺失时插入目标 `version` 字段。

对于 manifest 内部依赖，adapter 必须同时提供依赖类别与原生版本约束。所有类别统一参与
`WorkspaceGraph` 排序；`ReleasePlanner` 的传播 allowlist 则按 6.3 节执行。首版只将 Rust
`[dependencies]` 视为约束感知的传播依赖：计划新版本不满足约束时，将依赖方自动加入发布闭包并规划
manifest 版本更新；约束仍满足时不单独发布依赖方。`dev-dependencies`、`build-dependencies`、
Node peer/optional 以及 Node.js、Python、C++ 的其他 manifest 边不自动传播；需要依赖发布即重新构建
或发布的关系必须用 `depends-on` 显式声明。

Adapter 不可以：

- 持有或读取全局 `Context`；
- 决定全局包顺序；
- 直接写文件；
- 执行子进程；
- 访问 registry 或 GitHub；
- 处理 dry-run。

#### C++ workspace 与内部依赖（首版）

阶段 0 为 CMake 项目采用可静态分析的最小规则，作为后续 `WorkspaceGraph` 的 fixture 基线：

- 工作区根为包含 `project(... VERSION ...)` 的根 `CMakeLists.txt`；
- 根文件中每个字面量 `add_subdirectory(path)` 声明一个直接成员；成员目录包含带版本的 `project(...)` 时，根项目与该成员均被发现为 package；
- 不解析变量、generator expression、下载或运行时生成的子目录；
- C++ adapter 迁移后递归跟随项目根内各 `CMakeLists.txt` 的字面量
  `add_subdirectory(path)`。中间目录可以只用于分组而不声明 package；每个包含
  `project(... VERSION ...)` 的可达目录均被发现为 package。遍历按规范化相对路径稳定
  排序并去重，指向项目根外的路径必须报错，不得读取或发现外部项目；
- 当成员项目的 `CMakeLists.txt` 以自身 `project` 名称作为第一个参数调用 `target_link_libraries(...)`，且后续参数中出现同一工作区内另一个 `project` 名称时，建立该内部依赖边；`PUBLIC`、`PRIVATE` 与 `INTERFACE` 在阶段 0 均只影响排序，不改变版本传播；
- 未匹配到上述静态形式的 CMake target 关系不推导为内部依赖，用户可在后续的可选 `depends-on` 配置中显式声明。

该规则只用于发现和依赖排序；版本写入、发布传播和跨生态依赖仍由后续 `WorkspaceGraph` 与 `ReleasePlanner` 定义。

### 7.3 跨生态依赖

某些跨生态依赖无法从 manifest 自动推导，例如 Node.js 包使用本地 Rust binding 产物。因此配置需要提供可选显式边：

```toml
[packages.node-binding]
path = "packages/node"
resolver = "nodejs"
depends-on = ["rust-core"]
```

该字段在新实现稳定后作为向后兼容的可选配置引入，不阻塞第一阶段迁移。

`depends-on` 中的值必须引用配置中的稳定 `PackageId`，允许跨生态引用。每条显式边转换为
`source = Config`、`kind = Unspecified`、无版本约束的内部依赖：

- 与 manifest 内部依赖共同参与拓扑排序、未知目标校验和依赖环检测；
- 依赖 package 发生任意版本发布时，触发 dependent 的 patch 发布，不受 dependent 所属生态限制；
- 不将该传播规则扩大到 Node.js、Python 或 C++ manifest 依赖；这些依赖种类的传播策略仍按阶段 4
  的独立任务定义；
- 新增 package 或执行 `config sync` 时不自动生成 `depends-on`，已有字段和顺序必须保留。

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

每个 `FileEdit` 必须显式声明其规划时的目标状态：已有文件使用源内容哈希，首次创建文件则要求目标在应用时仍不存在。执行器必须在写入任何临时文件前验证全部前置条件；不得将“不存在”隐式当作空文件，否则并发创建的 changelog 可能被无意覆盖。

Post-version 子进程不可能真正纳入跨进程事务。在执行前应尽可能完成所有静态验证。命令失败时保留已写入的文件和 changeset，不尝试不可靠的自动回滚；`ApplyReport` 清楚报告已完成文件、失败命令和未消费 changeset，供用户修复命令后重试。

生产代码不得使用会 panic 的 `unwrap()` 处理外部输入、文件系统、配置或领域查询结果；这些失败必须通过类型化错误或可传播错误返回。只有已由类型、构造器或同一函数内穷尽分支证明的内部不变量可以使用 `expect()`，且消息必须说明该不变量；不得把可恢复失败标记为不变量。测试代码中的断言性使用不属于运行时失败路径。

`--dry-run` 只执行规划和验证并渲染结果，不调用写入器或命令运行器。

## 12. Changelog 设计

Changelog 生成应分为两部分：

1. 可选元数据收集：从 Git 和 Forge 获取 commit、PR 和 author；
2. 纯格式化：根据 changeset、版本和元数据生成 Markdown。

格式化器接收 `ChangelogContext` 或其序列化后的模板视图，不应持有 `Context`、`git2::Repository` 或自行创建 `Octocrab`。GitHub PR 元数据查询失败时记录诊断并降级为不含 PR 信息的 changelog，不中断 `version`；该策略必须在收集层实现，不能成为格式化函数的隐式行为。

每个用户 changeset 必须渲染为一个独立的 Markdown 列表项。只有单行 summary 时，连续列表项保持紧凑。summary 含后续行时，每个非空后续行必须以一个空行分隔，并使用四个空格缩进为同一列表项内的独立段落；summary 中已有的空行只作为段落分隔，不得生成仅含缩进空格的伪段落。最后一个后续行后也必须保留一个空行，再渲染下一个列表项，避免依赖 Markdown 渲染器对 soft break 的不同处理。必须保留所有非空行的原有顺序。commit 与 PR 元数据属于该 changeset 的列表项标题，必须附在第一行，不能落到最后一个续行之后。例如：

```markdown
- First line ([#42](https://example.com/pull/42) by @author)

    Second line

    Third line

- Another changeset
```

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
smif config channel set alpha --package semifold --bump preserve
smif config channel clear --package semifold
```

`set` 只接受非空的命名通道；`stable` 是保留值，恢复 stable 必须使用 `clear`。命令必须指定一个或多个 `--package <PackageId>`，或显式指定 `--all`，两者不能同时使用。未知 package 是错误，避免因拼写失误产生无效配置。

`set` 修改目标 package 的 `channel` 字段，并可通过可选的
`--bump <preserve|patch|minor|major>` 记录一次性 `channel-bump` 转换策略。未指定
`--bump` 时保持现有行为，由 changeset 的最高 bump 决定首次进入通道的稳定基准。
`preserve` 保留当前稳定基准，`patch`、`minor` 和 `major` 则显式覆盖该基准的提升级别。
例如 `0.1.0` 进入 `alpha` 时分别得到 `0.1.0-alpha.0`、`0.1.1-alpha.0`、
`0.2.0-alpha.0` 和 `1.0.0-alpha.0`。该策略仅写入本次 `set` 需要覆盖的目标 package；
已处于请求 channel 的 package 不会因 `--bump` 单独发生变化。

`channel-bump` 只在 package 当前为 stable 且下一次 `version` 首次进入配置的命名通道时生效。
成功应用 manifest、changelog 和 post-version 后，`version` 从对应 package table 删除该字段；
dry-run、规划失败、文件应用失败或 post-version 失败都不得消费它。通道内后续发布仍只推进序号。

`clear` 删除 `channel` 和未消费的 `channel-bump`，使 package 回到缺省 stable 状态，且不接受
`--bump`。二者都使用 `toml_edit::DocumentMut` 与原子写回，保留目标 table 的其他字段、注释和所有非目标 package。无实际变化时不得写入文件。

全局 `--dry-run` 只输出将修改的 package 而不写入；`--check` 断言目标已处于请求状态，存在需要修改的 package 时返回非零。JSON 配置不受支持。

### 13.3 同步计划

与 release 流程相同，配置更新也采用 Plan/Validate/Apply：

```rust
pub struct ConfigSyncPlan {
    pub config_path: Utf8PathBuf,
    pub added: Vec<DiscoveredPackage>,
    pub missing: Vec<ConfiguredPackage>,
    pub renamed: Vec<PackageRename>,
    pub moved: Vec<PackageMove>,
    pub conflicts: Vec<ConfigConflict>,
    pub warnings: Vec<ConfigSyncWarning>,
}
```

首版领域输入与分类结果使用以下结构：

```rust
pub struct ConfiguredPackage {
    pub id: PackageId,
    pub ecosystem: Ecosystem,
    pub path: Utf8PathBuf,
}

pub struct DiscoveredPackage {
    pub id: PackageId,
    pub ecosystem: Ecosystem,
    pub path: Utf8PathBuf,
}

pub struct PackageRename {
    pub from: PackageId,
    pub to: PackageId,
    pub ecosystem: Ecosystem,
    pub path: Utf8PathBuf,
}

pub struct PackageMove {
    pub package: PackageId,
    pub ecosystem: Ecosystem,
    pub from: Utf8PathBuf,
    pub to: Utf8PathBuf,
}

pub enum ConfigConflict {
    AmbiguousMatch {
        configured: Vec<ConfiguredPackage>,
        discovered: Vec<DiscoveredPackage>,
    },
    ResolverChanged {
        configured: ConfiguredPackage,
        discovered: DiscoveredPackage,
    },
}

pub struct ChangesetReference {
    pub changeset: ChangesetId,
    pub packages: BTreeSet<PackageId>,
}

pub enum ConfigSyncWarning {
    ChangesetReferencesRenamedPackage {
        changeset: ChangesetId,
        from: PackageId,
        to: PackageId,
    },
}
```

`ConfigSyncPlanner` 是纯函数式领域服务，接收 config 路径、已配置 package 快照、完整 discovery 快照和未消费 changeset 引用。package path 在进入 planner 前必须已转换为相对项目根目录的规范化 UTF-8 路径；路径规范化失败属于应用层输入错误，不伪装为同步冲突。planner 不读取文件、不决定 `--prune`，也不修改配置。

输出使用 `missing` 而不是 `removed`：它只表示配置项未被发现；是否删除由应用阶段结合完整扫描状态和 `--prune` 决定。rename 命中旧 package id 的未消费 changeset 时生成 `ChangesetReferencesRenamedPackage` warning，但不修改 changeset。所有结果按 package id、路径、生态和 changeset id 稳定排序。同一输入无论迭代顺序如何都必须产生相同计划。

discovery 从 manifest name 生成的 `PackageId` 只是新增和 rename 的默认建议，不得覆盖已经配置的稳定
身份。匹配前先识别同一生态内的重复建议；这代表 manifest name 本身有歧义，直接分类为
`AmbiguousMatch`。随后严格按以下顺序匹配：

1. 第一轮匹配相同 ecosystem 与相同规范化 path。默认建议唯一且 package id 不同时分类为 rename；
   当同一默认建议被不同生态复用时，保留现有配置的 `PackageId`，不产生 rename；
2. 对仍未匹配的 discovery 建议检查全局唯一性。多个未匹配 package 竞争同一建议，或建议已被第一轮
   匹配的另一个 package 占用时，分类为 `AmbiguousMatch`，不得自动生成 namespace；
3. 第二轮在剩余项中匹配相同 `PackageId` 与相同 ecosystem，path 不同时分类为 move；
4. 相同路径或相同 `PackageId` 的 ecosystem 改变分类为 `ResolverChanged`。

任一轮存在多对一或一对多候选时，将全部相关项分类为 `AmbiguousMatch`，不得同时把它们报告为
added 或 missing。剩余 discovery 项为 added，剩余 config 项为 missing。

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

1. resolver 与规范化 package path 完全相同：视为同一个包；manifest name 改变且默认
   `PackageId` 建议唯一时识别为 rename，跨生态同名导致建议冲突时保留已配置 ID；
2. `PackageId` 相同：path 改变时识别为 move；discovery 的默认 `PackageId` 从 manifest name 派生；
3. 名称和路径均不匹配：视为新增和缺失；
4. 多个包同时命中同一候选：产生冲突，不自动修改。

规范化路径必须：

- 相对于项目根目录；
- 使用统一分隔符；
- 移除 `.` 和可安全消解的 `..`；
- 不跟随项目根目录外的符号链接。

应用层提供共享的 `PackagePathNormalizer`，供 workspace 快照桥接、`init` 和 `config sync` 使用：

```rust
pub fn normalize_package_path(
    project_root: &Path,
    package_path: &Path,
) -> Result<Utf8PathBuf, PackagePathError>;
```

`project_root` 必须是已存在的绝对目录；`package_path` 可以是绝对路径或相对于项目根目录的路径。实现先做不访问文件系统的词法规范化，再检查最深的已存在祖先路径：如果符号链接解析后的祖先位于项目根目录之外则拒绝该输入，但返回值仍保留项目内的词法相对路径，不改写为符号链接目标。尚不存在但位于项目根目录内的 package path 可以被规范化，以便配置中的 missing package 参与同步。项目根 package 统一表示为 `.`，序列化路径统一使用 `/` 分隔符；无法表示为 UTF-8 或逃逸项目根目录时返回结构化错误。

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

新条目必须采用确定性顺序插入：在 `[packages]` 内按 `PackageId` 排序新增条目，但不重排现有条目，避免产生大面积无意义 diff。

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

`ReleaseChannel` 是发布流程概念，不是 SemVer 的 `Prerelease` 概念。核心仅处理通道状态与序号推进；为保持现有 `ReleasePlan` 的统一排序和序列化，它暂以 SemVer 形状保存过渡版本值，但这个值不是 manifest 的最终版本字符串。`EcosystemAdapter::encode_version()` 必须在 `plan_edits()` 前将其验证并编码为所属生态的版本文本，所有 package version 与内部依赖版本编辑都必须使用该结果：

| 生态 | `channel = "alpha"` 的一种编码 | `channel = "post"` 的一种编码 |
| --- | --- | --- |
| Rust / Node（SemVer） | `1.0.0-alpha.1` | `1.0.0-post.1` |
| Python（PEP 440） | `1.0.0a1` 或项目约定格式 | `1.0.0.post1` |
| CMake / vcpkg | 不支持命名通道；`project(VERSION)` 只接受稳定数字版本 | 不支持 |

Rust 和 Node adapter 直接使用合法的 SemVer 文本。Python adapter 将 `alpha.N`、`beta.N`、`rc.N` 和 `post.N` 分别编码为 PEP 440 的 `aN`、`bN`、`rcN` 和 `.postN`，并在 inspection 时将这四种 PEP 440 格式还原为领域的过渡值；其他命名通道在 Python 规划时失败。CMake adapter 拒绝任何含 prerelease 的过渡值，避免生成 CMake 不接受的 `project(VERSION)`。因此，`channel` 的字符串不在 core 中按各生态规范限制；adapter 必须在规划阶段验证其能否表示并在无法表示时返回结构化错误。

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
    pub fn apply(
        &mut self,
        plan: &ConfigSyncPlan,
        prune_missing: bool,
    ) -> Result<(), ConfigEditError>;
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

`apply` 在存在任何 `conflicts` 时必须在修改文档前返回错误。`prune_missing = false` 时仅应用 `added`、`renamed` 与 `moved`，`missing` 保持为报告项且不得删除 table；`prune_missing = true` 时删除 `missing` 对应的 package table。CLI 只可在完整扫描成功、未排除任何 resolver 且计划无冲突时传入 `true`，随后使用临时文件和 rename 原子写回。

### 13.8 与 `init` 的关系

`init` 和 `config sync` 应共享：

- resolver registry；
- package discovery；
- package path 规范化；
- 默认 package 配置生成；
- 冲突诊断。

迁移期间由应用层提供统一发现入口，隐藏 resolver factory 与领域 ecosystem 的映射：

```rust
pub struct PackageDiscovery {
    pub resolvers: Vec<ResolverType>,
    pub packages: Vec<DiscoveredPackage>,
}

pub struct PackageDiscoveryService {
    registry: ResolverRegistry,
}

impl PackageDiscoveryService {
    pub fn discover(
        &self,
        project_root: &Path,
        resolvers: &[ResolverType],
    ) -> Result<PackageDiscovery, PackageDiscoveryError>;
}
```

resolver 选择先按类型稳定排序并去重。服务通过 registry 创建现有 resolver，调用发现接口，将 manifest name 作为默认 `PackageId` 建议，并使用共享 `PackagePathNormalizer` 生成规范化路径。`PackageDiscovery.packages` 按 `PackageId`、ecosystem 和 path 稳定排序；重复建议保留在发现快照中。`ConfigSyncPlanner` 可用相同 ecosystem 与 path 将跨生态同名结果绑定到已有稳定 ID；首次 `init` 或未配置的多个新增 package 仍竞争同一建议时产生多义冲突并停止，不猜测 namespace。

一次 discovery 只有“完整成功”或“失败”两种结果：任一所选 resolver 的 glob 遍历、manifest 读取、package 解析或路径规范化失败时，整个调用返回结构化错误，不得返回看似完整的部分快照。未选择的 resolver 不属于扫描范围；应用层据此禁止在部分 resolver 模式下 prune 其他生态的配置。现有 `resolve_all` 中记录 warning 后跳过损坏 package 的路径必须改为传播错误，避免把扫描失败误判为 package 已删除。

`plan_config_sync` 的应用层桥接通过 `ConfigSyncScope` 选择 resolver：未指定 `--resolver` 时选择配置中全部已启用的 resolver；显式选择必须是已启用 resolver 的子集，否则返回 `ResolverNotEnabled`。scope 将这些生态的 `[packages]` table 转换为 `ConfiguredPackage`，调用统一 discovery，并把未消费 changeset 转换为 `ChangesetReference` 后交给 `ConfigSyncPlanner`。未被本次 resolver 范围覆盖的配置项不得进入 `missing`。CLI 的显式 `--resolver` 只改变 scope，不复制快照转换或匹配逻辑；只有 scope 覆盖全部已启用 resolver 时才允许 `--prune`。

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
- `config channel set --bump` 仅为实际切换 channel 的目标 package 写入一次性策略，并覆盖 preserve/patch/minor/major 的首次进入通道版本。
- `version` 仅在成功完成后清理已使用的 `channel-bump`；dry-run 和各失败路径保留它。

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

#### 现有 resolver 的临时转换

阶段 1 在 ecosystem adapter 完成拆分前，由 engine/应用层将现有 resolver 数据临时转换为 `PackageSnapshot`：

- `.changes/config.toml` 中的 package key 作为 `PackageId`，manifest 声明的名称保留为 `manifest_name`；
- resolver 除 `ResolvedPackage` 外，临时暴露 manifest 依赖的名称、`DependencyKind` 与原始版本约束；resolver 不负责将依赖名称解释为 `PackageId`；
- 应用层先解析所有已配置 package，再按 `(Ecosystem, manifest_name)` 建立到 `PackageId` 的映射；同一生态出现重复 manifest name 时停止转换，避免产生歧义；
- 只有在同一生态中唯一匹配到已配置 package 的依赖才转换为 `source = Manifest` 的内部 `Dependency`，未匹配项视为外部依赖，不进入 `WorkspaceGraph`；首个切片不推断跨生态依赖，后续由 `source = Config` 的显式 `depends-on` 合并；
- package 路径必须能转换为 UTF-8 相对路径；`private` 映射为 `publishable = false`；
- 转换完成后统一调用 `WorkspaceGraph` 校验并排序，不再调用各 resolver 的 `sort_packages()` 产生新架构顺序。

该转换是迁移桥接层，不作为最终 `EcosystemAdapter` 接口；切换 `status` 后再逐步删除 resolver 内的旧排序职责。

阶段 4 收敛完成后，`EcosystemAdapter` 是生态能力唯一的多态接口；旧 `Resolver` trait、
`ResolvedPackage`、`ResolvedDependency`、`create_resolver()` 以及
`Context::create_resolver()` 全部删除。discovery、workspace 加载、publish inspect 和 ecosystem
fixtures 直接消费 `PackageInspection` 与 `ManifestDependency`，不再把 adapter 输出转换回旧模型。
`ResolverType` 仅保留为配置文件中的生态选择器，由 application registry 映射到 adapter。

发布 pre-check 模板继续兼容现有 `package.name`、`package.version`、`package.path` 与
`package.private` 字段，但该视图由 application 从 `PackageInspection` 构造，不要求 ecosystem
adapter 暴露旧 `ResolvedPackage`。

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

- 阶段开始时先移除旧 `version` 的 Rust 专属版本规划路径：混合生态工作区中任何受 changeset 影响的 package 都必须从同一个 `ReleasePlan` 取得发布顺序和目标版本，不能因旧的 Rust-only 当前版本 map 缺项而 panic；
- 引入 `FileEdit` 和 `VersionMap`；
- 改造 Rust 和 Node.js resolver，使其返回修改内容而不直接写入；
- 实现文件修改验证与统一应用；
- 删除 Rust/Node.js 对 `Context.version_bumps` 的依赖；
- 将 changelog 写入纳入同一执行过程。

完成条件：`status`、`version --dry-run` 和 `version` 使用相同计划；包含 Rust 与 Node.js package 的 changeset 能完成规划且不 panic。跨生态自动传播仍只在阶段 4 合并显式 `depends-on` 边后启用。

### 阶段 4：完成 ecosystem adapter 迁移

目标：移除 resolver 的全局职责。

- 迁移 Python 和 C++；
- 删除 `Resolver::sort_packages()`；
- 删除 `Resolver::publish()`；
- 删除旧 `Resolver` trait、桥接数据类型和 factory；
- 删除 adapter 对 `Context` 和 dry-run 的依赖；
- 先将现有 `semifold-resolver` 的生态能力收敛到唯一 `EcosystemAdapter` 接口；crate
  重命名为 `semifold-ecosystems` 是独立的 package 元数据迁移，不阻塞接口收敛完成。

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

1. [已决定] 运行时内部依赖的新版本不满足依赖方 manifest 约束时，才自动触发依赖方 patch bump；显式 changeset 的更高 bump 优先。约束仍满足时不自动发布依赖方。
2. [已决定] 首版 Rust 仅 `[dependencies]` 参与自动版本传播；`dev-dependencies` 与 `build-dependencies` 不自动传播。
3. [已决定] 所有内部依赖类别参与拓扑排序；首版 manifest 自动传播仅支持 Rust runtime，
   development、build、peer、optional 及其他生态 manifest 依赖不自动传播，需要时使用
   `depends-on`。
4. [已决定] `PackageId` 全局唯一但不自动添加 namespace；跨生态同名 manifest 由已有配置的稳定 ID 区分，首次发现无法唯一落盘时报告冲突。
5. [已决定] post-version 命令失败时保留已写入文件和 changeset，不自动回滚；输出包含已完成文件、失败命令和未消费 changeset 的结构化恢复指引。
6. [已决定] GitHub PR 元数据查询失败时降级为无 PR 信息的 changelog，不中断 `version`，并保留可诊断的收集错误。
7. `config sync` 是否需要在后续版本支持 JSON 配置，还是正式将可编辑配置限定为 TOML。
8. 未启用 resolver 但发现对应生态 manifest 时，是提示用户启用，还是允许 `--resolver` 自动创建默认 resolver 配置。
9. rename 后是否提供独立 `--rewrite-changesets` 选项更新尚未消费的 changeset，默认行为仍是不修改。
10. 一个 package 是否允许属于多个 `ReleaseUnit`；若允许，如何界定它们的发布和版本修改冲突。
11. `Fingerprint` identity 的稳定输入、可见格式和适用场景。

### 19.1 低优先级优化：首次发布状态

manifest 中的版本不足以可靠判断 package 是否已经发布。长期可以引入 `PackageReleaseState`，由 engine 通过 registry package metadata 获取 `Unpublished` 或 `Published` 状态，再作为纯 `ReleasePlanner` 的输入。该查询与发布前“目标版本是否已存在”的 `version_exists` pre-check 是两个不同语义；网络失败、鉴权失败或无 registry 配置时不得推断为 `Unpublished`。

该能力不阻塞当前架构重构，具体状态模型、离线行为、registry port 和各生态实现留待后续决策。`semifold-core` 首次发布暂时将 manifest 版本设为 `0.0.0`，使现有 minor changeset 在 alpha 通道生成 `0.1.0-alpha.0`，在 stable 通道生成 `0.1.0`。这是当前仓库的过渡措施，不定义为所有新 package 的长期通用规则。

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
