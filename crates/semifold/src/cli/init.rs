use std::{collections::BTreeMap, path::PathBuf};

use clap::{Args, ValueEnum};
use inquire::{Confirm, MultiSelect, Select, Text};
use rust_i18n::t;
use semifold_resolver::{
    config::{self, BranchesConfig, PackageConfig, PreCheckConfig, PublishConfig, ResolverConfig},
    context,
    error::ResolveError,
    resolver::{self, Resolver, ResolverType as ResolverTypeEnum},
};

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum ResolverType {
    Rust,
}

impl From<ResolverType> for resolver::ResolverType {
    fn from(value: ResolverType) -> Self {
        match value {
            ResolverType::Rust => resolver::ResolverType::Rust,
        }
    }
}

impl std::fmt::Display for ResolverType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolverType::Rust => write!(f, "Rust"),
        }
    }
}

#[derive(rust_embed::Embed)]
#[folder = "assets"]
pub(crate) struct CIAsset;

#[derive(Debug, Args)]
pub(crate) struct Init {
    #[arg(short, long, default_value = ".changes")]
    pub target: Option<PathBuf>,
    #[arg(short, long)]
    pub resolvers: Vec<ResolverType>,
    #[arg(short, long, default_value_t = false)]
    pub force: bool,
    #[arg(long)]
    pub base_branch: Option<String>,
    #[arg(long)]
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
                        extra_headers: Some(BTreeMap::from_iter([
                            ("User-Agent".to_string(), format!("Semifold {}", env!("CARGO_PKG_VERSION"))),
                        ])),
                    },
                    prepublish: vec![PublishConfig {
                        command: "cargo".to_string(),
                        args: vec!["publish".to_string(), "--dry-run".to_string()].into(),
                    }],
                    publish: vec![PublishConfig {
                        command: "cargo".to_string(),
                        args: vec!["publish".to_string()].into(),
                    }],
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
            base: base_branch,
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
        let ci_asset = CIAsset::get("semifold-ci.yaml").unwrap();
        let status_ci_asset = CIAsset::get("status.yaml").unwrap();

        std::fs::write(
            target_dir.join(".github/workflows/semifold-ci.yaml"),
            ci_asset.data,
        )?;
        std::fs::write(
            target_dir.join(".github/workflows/status.yaml"),
            status_ci_asset.data,
        )?;
    }

    Ok(())
}
