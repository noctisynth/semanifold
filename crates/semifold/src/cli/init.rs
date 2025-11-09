use std::{collections::BTreeMap, path::PathBuf};

use clap::{Args, ValueEnum};
use inquire::{Confirm, MultiSelect, Select, Text};
use rust_i18n::t;
use semifold_resolver::{
    config::{
        self, BranchesConfig, CommandConfig, PackageConfig, PreCheckConfig, ResolverConfig,
        VersionMode,
    },
    context,
    error::ResolveError,
    resolver::{self, Resolver, ResolverType as ResolverTypeEnum},
};

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum ResolverType {
    Rust,
    Nodejs,
    Python,
}

impl From<ResolverType> for resolver::ResolverType {
    fn from(value: ResolverType) -> Self {
        match value {
            ResolverType::Rust => resolver::ResolverType::Rust,
            ResolverType::Nodejs => resolver::ResolverType::Nodejs,
            ResolverType::Python => resolver::ResolverType::Python,
        }
    }
}

impl std::fmt::Display for ResolverType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolverType::Rust => write!(f, "Rust"),
            ResolverType::Nodejs => write!(f, "Nodejs"),
            ResolverType::Python => write!(f, "Python"),
        }
    }
}

#[derive(rust_embed::Embed)]
#[folder = "assets"]
pub(crate) struct CIAsset;

#[derive(Debug, Args)]
pub(crate) struct Init {
    #[arg(short, long, default_value = ".changes", help = t!("cli.init.flags.target"))]
    pub target: Option<PathBuf>,
    #[arg(short, long, help = t!("cli.init.flags.resolvers"))]
    pub resolvers: Vec<ResolverType>,
    #[arg(short, long, default_value_t = false, help = t!("cli.init.flags.force"))]
    pub force: bool,
    #[arg(long, help = t!("cli.init.flags.base_branch"))]
    pub base_branch: Option<String>,
    #[arg(long, help = t!("cli.init.flags.release_branch"))]
    pub release_branch: Option<String>,
}

pub(crate) fn run(init: &Init, ctx: &context::Context) -> anyhow::Result<()> {
    if ctx.is_initialized() && !init.force {
        log::warn!("{}", t!("cli.init.already_initialized"));
        return Ok(());
    }

    const AVAILABLE_TARGETS: [&str; 2] = [".changes", ".changesets"];

    let mut target_dir = std::env::current_dir()?;
    if ctx.repo_root.is_some() && ctx.repo_root.as_ref().unwrap() != &target_dir {
        log::warn!("{}", t!("cli.init.not_repo_root"));
        if !Confirm::new(&t!("cli.init.continue"))
            .with_default(false)
            .prompt()?
        {
            log::warn!("{}", t!("cli.init.aborted"));
            return Ok(());
        }
        target_dir = ctx.repo_root.as_ref().unwrap().to_path_buf();
    }

    let target = if let Some(target) = &init.target {
        target_dir.join(target)
    } else {
        let target = Select::new(&t!("cli.init.target"), AVAILABLE_TARGETS.to_vec()).prompt()?;
        target_dir.join(target)
    };

    log::debug!("target: {}", target.display());

    let resolvers = if init.resolvers.is_empty() {
        MultiSelect::new(
            &t!("cli.init.resolvers"),
            ResolverType::value_variants().to_vec(),
        )
        .prompt()?
    } else {
        init.resolvers.clone()
    };
    let resolvers_config = BTreeMap::from_iter(resolvers.iter().map(|r| {
        match r {
            ResolverType::Rust => (
                ResolverTypeEnum::Rust,
                ResolverConfig {
                    pre_check: PreCheckConfig {
                        url:
                            "https://crates.io/api/v1/crates/{{ package.name }}/{{ package.version }}"
                                .to_string(),
                        extra_headers: BTreeMap::from_iter([
                            ("User-Agent".to_string(), format!("Semifold {}", env!("CARGO_PKG_VERSION"))),
                        ]),
                    },
                    prepublish: vec![],
                    publish: vec![CommandConfig {
                        command: "cargo".to_string(),
                        args: vec!["publish".to_string()].into(),
                        extra_env: BTreeMap::new(),
                        stdout: config::StdioType::Inherit,
                        stderr: config::StdioType::Inherit,
                        dry_run: None,
                    }],
                    post_version: vec![CommandConfig {
                        command: "cargo".to_string(),
                        args: vec!["generate-lockfile".to_string(), "--offline".to_string()].into(),
                        extra_env: BTreeMap::new(),
                        stdout: config::StdioType::Inherit,
                        stderr: config::StdioType::Inherit,
                        dry_run: Some(true),
                    }],
                },
            ),
            ResolverType::Nodejs => (
                ResolverTypeEnum::Nodejs,
                ResolverConfig {
                    pre_check: PreCheckConfig {
                        url:
                            "https://registry.npmjs.org/{{ package.name }}/{{ package.version }}"
                                .to_string(),
                        extra_headers: BTreeMap::new(),
                    },
                    prepublish: vec![],
                    publish: vec![CommandConfig {
                        command: "npm".to_string(),
                        args: vec!["publish".to_string(), "--provenance".to_string(), "--access".to_string(), "public".to_string()].into(),
                        extra_env: BTreeMap::new(),
                        stdout: config::StdioType::Inherit,
                        stderr: config::StdioType::Inherit,
                        dry_run: None,
                    }],
                    post_version: vec![]
                },
            ),
            ResolverType::Python => (
                ResolverTypeEnum::Python,
                ResolverConfig {
                    pre_check: PreCheckConfig {
                        url:
                            "https://pypi.org/pypi/{{ package.name }}/{{ package.version }}/json"
                                .to_string(),
                        extra_headers: BTreeMap::from_iter([
                            ("User-Agent".to_string(), format!("Semifold {}", env!("CARGO_PKG_VERSION"))),
                        ]),
                    },
                    prepublish: vec![],
                    publish: vec![],
                    post_version: vec![]
                },
            ),
        }
    }));

    log::debug!("resolvers: {resolvers:?}");

    let packages = resolvers
        .iter()
        .try_fold(BTreeMap::new(), |mut acc, name| match name {
            ResolverType::Rust => {
                let mut resolver = resolver::rust::RustResolver;
                let packages = resolver.resolve_all(&target_dir)?;
                packages.into_iter().for_each(|pkg| {
                    acc.entry(pkg.name.clone()).or_insert(PackageConfig {
                        path: pkg.path.clone(),
                        resolver: resolver::ResolverType::Rust,
                        version_mode: VersionMode::Semantic,
                        assets: vec![],
                    });
                });
                Ok::<_, ResolveError>(acc)
            }
            ResolverType::Nodejs => {
                let mut resolver = resolver::nodejs::NodejsResolver;
                let packages = resolver.resolve_all(&target_dir)?;
                packages.into_iter().for_each(|pkg| {
                    acc.entry(pkg.name.clone()).or_insert(PackageConfig {
                        path: pkg.path.clone(),
                        resolver: resolver::ResolverType::Nodejs,
                        version_mode: VersionMode::Semantic,
                        assets: vec![],
                    });
                });
                Ok::<_, ResolveError>(acc)
            }
            ResolverType::Python => {
                let mut resolver = resolver::python::PythonResolver;
                let packages = resolver.resolve_all(&target_dir)?;
                packages.into_iter().for_each(|pkg| {
                    acc.entry(pkg.name.clone()).or_insert(PackageConfig {
                        path: pkg.path.clone(),
                        resolver: resolver::ResolverType::Python,
                        version_mode: VersionMode::Semantic,
                        assets: vec![],
                    });
                });
                Ok::<_, ResolveError>(acc)
            }
        })?;

    log::debug!("packages: {packages:?}");

    let tags = if Confirm::new(&t!("cli.init.tags"))
        .with_default(true)
        .prompt()?
    {
        BTreeMap::from_iter([
            ("chore".to_string(), "Chores".to_string()),
            ("feat".to_string(), "New Features".to_string()),
            ("fix".to_string(), "Bug Fixes".to_string()),
            ("perf".to_string(), "Performance Improvements".to_string()),
            ("refactor".to_string(), "Refactors".to_string()),
        ])
    } else {
        BTreeMap::default()
    };

    let base_branch = if let Some(base_branch) = &init.base_branch {
        base_branch.clone()
    } else {
        Text::new(&t!("cli.init.base_branch"))
            .with_default("main")
            .prompt()?
    };

    let release_branch = if let Some(release_branch) = &init.release_branch {
        release_branch.clone()
    } else {
        Text::new(&t!("cli.init.release_branch"))
            .with_default("release")
            .prompt()?
    };

    let config = config::Config {
        branches: BranchesConfig {
            base: base_branch.clone(),
            release: release_branch,
        },
        tags,
        packages,
        resolver: resolvers_config,
    };

    let write_ci = Confirm::new(&t!("cli.init.write_ci"))
        .with_default(true)
        .prompt()?;

    if !target.exists() {
        std::fs::create_dir_all(&target)?;
    }
    config::save_config(&target.join("config.toml"), &config)?;
    if write_ci {
        if !target_dir.join(".github").exists() {
            std::fs::create_dir_all(target_dir.join(".github"))?;
        }
        let ci_asset = CIAsset::get("semifold-ci.yaml.jinja").unwrap();
        let status_ci_asset = CIAsset::get("semifold-status.yaml.jinja").unwrap();

        let ci_str = String::from_utf8_lossy(&ci_asset.data).to_string();
        let status_ci_str = String::from_utf8_lossy(&status_ci_asset.data).to_string();

        std::fs::write(
            target_dir.join(".github/workflows/semifold-ci.yaml"),
            minijinja::render!(&ci_str, base_branch => &base_branch),
        )?;
        std::fs::write(
            target_dir.join(".github/workflows/semifold-status.yaml"),
            minijinja::render!(&status_ci_str, base_branch => &base_branch),
        )?;
    }

    Ok(())
}
