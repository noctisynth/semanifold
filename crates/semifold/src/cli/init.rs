use std::{collections::BTreeMap, path::PathBuf};

use clap::{Parser, ValueEnum};
use inquire::{Confirm, MultiSelect, Select, Text};
use rust_i18n::t;
use semifold_engine::{
    InitOptions, InitWorkflowTemplates, ProjectLocation, SemifoldService, SystemDependencies,
};
use semifold_resolver::resolver::ResolverType;

#[derive(rust_embed::Embed)]
#[folder = "assets"]
pub(crate) struct CIAsset;

#[derive(Debug, Parser)]
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

pub(crate) fn run(init: &Init, location: &ProjectLocation) -> anyhow::Result<()> {
    if location.existing_config.is_some() && !init.force {
        log::warn!("{}", t!("cli.init.already_initialized"));
        return Ok(());
    }

    const AVAILABLE_TARGETS: [&str; 2] = [".changes", ".changesets"];

    let mut target_dir = std::env::current_dir()?;
    if location.root.as_std_path() != target_dir {
        log::warn!("{}", t!("cli.init.not_repo_root"));
        if !Confirm::new(&t!("cli.init.continue"))
            .with_default(false)
            .prompt()?
        {
            log::warn!("{}", t!("cli.init.aborted"));
            return Ok(());
        }
        target_dir = location.root.clone().into_std_path_buf();
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
    log::debug!("resolvers: {resolvers:?}");

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

    let write_ci = Confirm::new(&t!("cli.init.write_ci"))
        .with_default(true)
        .prompt()?;

    let workflows = if write_ci {
        let ci_asset = CIAsset::get("semifold-ci.yaml.jinja")
            .ok_or_else(|| anyhow::anyhow!(t!("cli.init.asset_missing", name = "semifold-ci")))?;
        let status_ci_asset = CIAsset::get("semifold-status.yaml.jinja").ok_or_else(|| {
            anyhow::anyhow!(t!("cli.init.asset_missing", name = "semifold-status"))
        })?;

        Some(InitWorkflowTemplates {
            release: String::from_utf8_lossy(&ci_asset.data).to_string(),
            status: String::from_utf8_lossy(&status_ci_asset.data).to_string(),
        })
    } else {
        None
    };
    let target = camino::Utf8PathBuf::from_path_buf(target).map_err(|path| {
        anyhow::anyhow!(t!(
            "cli.init.non_utf8_target",
            path = path.to_string_lossy()
        ))
    })?;
    let service = SemifoldService::new(SystemDependencies);
    let plan = service.plan_init(
        location,
        InitOptions {
            target,
            resolvers,
            tags,
            base_branch,
            release_branch,
            application_version: env!("CARGO_PKG_VERSION").to_string(),
            workflows,
        },
    )?;
    service.apply_init(&plan)?;

    Ok(())
}
