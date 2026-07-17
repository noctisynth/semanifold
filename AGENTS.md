# Semifold 项目上下文

## 权威文档

- 技术 PRD：[docs/prd/rust-architecture-redesign.md](docs/prd/rust-architecture-redesign.md)
- 实施差异清单：[TODO.md](TODO.md)

PRD 是需求、架构和设计的技术事实来源。开始任何实现前，先检查 PRD 是否已表达该需求；如果需求或设计发生变化，先更新 PRD，再修改代码。

`TODO.md` 只记录 PRD 与当前代码之间的未完成差异：

- PRD 领先于代码时，新增或更新对应 TODO；
- 完成并验证 PRD 中的一项能力后，更新或勾选对应 TODO；
- 不要将 TODO 作为独立需求来源，也不要让 TODO 与 PRD 冲突。

## Rust 与 Cargo 规则

- 禁止手动编辑任何 `Cargo.toml`。
- 新增或移除依赖、crate、workspace member 或 crate 元数据时，必须使用 Cargo CLI，例如 `cargo add`、`cargo remove`、`cargo new` 或 `cargo init`。
- 如果 Cargo CLI 无法表达变更、命令失败，或预期结果不明确，立即停止并向用户确认；不得通过手动编辑 `Cargo.toml` 绕过。

## 实施约束

- 未经用户明确要求，不直接修改代码或配置文件。
- 领域设计优先：先生成或更新计划、接口和测试策略，再引入副作用。
- 新设计中不恢复全局可变万能 `Context`；渲染使用不可变、按作用域构造的 `ReleaseContext`、`PackageContext`、`ChangelogContext` 与 `TemplateContext`。

## 国际化（i18n）

- 所有面向用户的 CLI 文案（命令描述、flag help、成功/错误/状态输出和交互提示）必须通过 `rust-i18n` 的 `t!` 宏提供；不得在 Rust 源码中新增硬编码的自然语言文本。
- 实现或修改用户可见文案时，必须同步更新 `crates/semifold/locales/en.toml` 中对应的 locale key，并遵循现有 `[cli.*]` 的分组结构。
- 在提交实现前，检查新增或修改的用户可见路径是否具有 locale key；测试断言可使用必要的固定字符串，但不应将其作为生产文案来源。
