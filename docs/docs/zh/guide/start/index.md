# 介绍

Semifold 是下一代跨语言单体仓库版本管理和发布工具，它提供了一致的版本管理和发布流程，并支持跨语言的单体仓库。

现在，跨语言的单体仓库已经变得越来越流行。很多时候，当项目主体语言不能满足项目需求时，开发者会选择使用其他语言来实现项目的某些功能。例如，一个仓库中可能包含一个 Rust 库和一个 Node.js 包，开发者使用`napi-rs`为 Node.js 生成 Rust 绑定来提升应用性能。

Semifold 帮助开发团队管理跨语言仓库的版本、变更日志和发布流程，确保跨语言仓库的版本管理和发布流程一致、自动化、零痛苦。无论你正在构建跨语言的包、工具库、应用还是服务，Semifold 都使你的发布流水线干净可靠。

Semifold 提供`smif`和`semifold`两个命令行工具，`smif`是`semifold`的别名，它们提供了一致的命令行接口。

## 为什么选择 Semifold？

Semifold 不同于 Changesets 和 Covector 这些已有的 Monorepo 版本发布管理工具，Semifold 专注于跨语言仓库并提供一致的版本管理和发布流程。Changesets 主要关注 JavaScript 和 npm 仓库，而 Covector 则相对通用，但强依赖于 Node.js 运行时环境，同时难以负担复杂的工作区环境。

Semifold 提供针对 Rust、Node.js、Python 等语言的工作区解析器，自动处理工作区中的跨语言依赖关系，确保版本管理和发布流程的一致性。对于未受支持的语言，Semifold 提供了灵活的扩展机制，允许开发者自行配置工作区。

由于 Semifold 使用 Rust 编写，这使得 Semifold 可以提供独立的可执行文件，无需额外的运行时环境配置。除此之外，Semifold 还发布在 [crates.io](https://crates.io/crates/semifold)、[Npm Registry](https://www.npmjs.com/package/semifold) 和 [PyPI](https://pypi.org/project/semifold/) 上，方便开发者在不同的项目中使用，避免额外的环境配置。
