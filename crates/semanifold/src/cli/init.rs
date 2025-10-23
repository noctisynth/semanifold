use std::{collections::BTreeMap, path::PathBuf};

use clap::Args;
use inquire::{Confirm, Select};
use semanifold_resolver::{
    config::{self, BranchesConfig, PackageConfig},
    context,
    error::ResolveError,
    resolver::{self, Resolver},
};

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum ResolverType {
    Rust,
}

impl std::fmt::Display for ResolverType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolverType::Rust => write!(f, "rust"),
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct Init {
    #[arg(short, long, default_value = ".changes")]
    pub target: Option<PathBuf>,
    #[arg(short, long, default_value = "rust")]
    pub resolvers: Vec<ResolverType>,
}

pub(crate) fn run(init: &Init, ctx: &context::Context) -> anyhow::Result<()> {
    if ctx.is_initialized() {
        log::warn!("Semanifold is already initialized.");
        return Ok(());
    }

    const AVAILABLE_TARGETS: [&str; 2] = [".changes", ".changesets"];

    let current_dir = std::env::current_dir()?;
    let target = if let Some(target) = &init.target {
        current_dir.join(target)
    } else {
        let target =
            Select::new("What is the target directory?", AVAILABLE_TARGETS.to_vec()).prompt()?;
        current_dir.join(target)
    };

    log::debug!("target: {}", target.display());

    let resolvers = if init.resolvers.is_empty() {
        vec![ResolverType::Rust]
    } else {
        init.resolvers.clone()
    };

    log::debug!("resolvers: {resolvers:?}");

    let packages = resolvers
        .iter()
        .try_fold(BTreeMap::new(), |mut acc, name| match name {
            ResolverType::Rust => {
                let mut resolver = resolver::rust::RustResolver;
                let packages = resolver.resolve_all(&current_dir)?;
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

    let tags = if Confirm::new("Add default tags to config?")
        .with_default(true)
        .prompt()?
    {
        BTreeMap::from_iter([
            ("chore".to_string(), "Chore".to_string()),
            ("feat".to_string(), "New Feature".to_string()),
            ("fix".to_string(), "Bug Fix".to_string()),
            ("perf".to_string(), "Performance Improvement".to_string()),
            ("refactor".to_string(), "Refactor".to_string()),
        ])
    } else {
        BTreeMap::default()
    };

    let config = config::Config {
        branches: BranchesConfig {
            base: "main".to_string(),
            release: "release".to_string(),
        },
        tags,
        packages,
        resolver: BTreeMap::new(),
    };

    if !target.exists() {
        std::fs::create_dir_all(&target)?;
    }

    config::save_config(&target.join("config.toml"), &config)?;

    Ok(())
}
