use std::{collections::BTreeMap, path::PathBuf};

use clap::{Parser, ValueEnum};
use inquire::{Confirm, MultiSelect, Select, Text};
use rust_i18n::t;
use semifold_engine::{
    InitOptions, InitWorkflowTemplates, ProjectLocation, SemifoldService, SystemDependencies,
};
use semifold_resolver::resolver::ResolverType;

use crate::cli::{
    require_interactive,
    terminal::{StepOutcome, Terminal},
};

#[derive(rust_embed::Embed)]
#[folder = "assets"]
pub(crate) struct CIAsset;

#[derive(Debug, Parser)]
pub(crate) struct Init {
    #[arg(short, long, default_value = ".changes", help = t!("cli.init.flags.target"))]
    pub target: Option<PathBuf>,
    #[arg(short, long, help = t!("cli.init.flags.resolvers"))]
    pub resolvers: Vec<ResolverType>,
    #[arg(long, conflicts_with = "resolvers", help = t!("cli.init.flags.no_resolvers"))]
    pub no_resolvers: bool,
    #[arg(short, long, default_value_t = false, help = t!("cli.init.flags.force"))]
    pub force: bool,
    #[arg(long, help = t!("cli.init.flags.base_branch"))]
    pub base_branch: Option<String>,
    #[arg(long, help = t!("cli.init.flags.release_branch"))]
    pub release_branch: Option<String>,
    #[arg(long, conflicts_with = "no_default_tags", help = t!("cli.init.flags.default_tags"))]
    pub default_tags: bool,
    #[arg(long, conflicts_with = "default_tags", help = t!("cli.init.flags.no_default_tags"))]
    pub no_default_tags: bool,
    #[arg(long, conflicts_with = "no_github_actions", help = t!("cli.init.flags.github_actions"))]
    pub github_actions: bool,
    #[arg(long, conflicts_with = "github_actions", help = t!("cli.init.flags.no_github_actions"))]
    pub no_github_actions: bool,
    #[arg(long, help = t!("cli.init.flags.allow_non_root"))]
    pub allow_non_root: bool,
}

pub(crate) fn run(init: &Init, location: &ProjectLocation) -> anyhow::Result<()> {
    let terminal = Terminal::detect();
    terminal.heading(&t!("cli.init.heading"));
    if location.existing_config.is_some() && !init.force {
        terminal.summary(StepOutcome::Skipped, &t!("cli.init.already_initialized"));
        return Ok(());
    }

    const AVAILABLE_TARGETS: [&str; 2] = [".changes", ".changesets"];

    let mut target_dir = std::env::current_dir()?;
    if location.root.as_std_path() != target_dir {
        terminal.warning(&t!("cli.init.not_repo_root"));
        if !init.allow_non_root {
            require_interactive(&t!("cli.init.continue"), "--allow-non-root")?;
            if !Confirm::new(&t!("cli.init.continue"))
                .with_default(false)
                .prompt()?
            {
                terminal.summary(StepOutcome::Skipped, &t!("cli.init.aborted"));
                return Ok(());
            }
        }
        target_dir = location.root.clone().into_std_path_buf();
    }

    let target = if let Some(target) = &init.target {
        target_dir.join(target)
    } else {
        require_interactive(&t!("cli.init.target"), "--target")?;
        let target = Select::new(&t!("cli.init.target"), AVAILABLE_TARGETS.to_vec()).prompt()?;
        target_dir.join(target)
    };

    log::debug!("target: {}", target.display());

    let resolvers = if init.resolvers.is_empty() && !init.no_resolvers {
        require_interactive(&t!("cli.init.resolvers"), "--resolvers or --no-resolvers")?;
        MultiSelect::new(
            &t!("cli.init.resolvers"),
            ResolverType::value_variants().to_vec(),
        )
        .prompt()?
    } else {
        init.resolvers.clone()
    };
    log::debug!("resolvers: {resolvers:?}");

    let use_default_tags = if init.default_tags {
        true
    } else if init.no_default_tags {
        false
    } else {
        require_interactive(&t!("cli.init.tags"), "--default-tags or --no-default-tags")?;
        Confirm::new(&t!("cli.init.tags"))
            .with_default(true)
            .prompt()?
    };
    let tags = if use_default_tags {
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
        require_interactive(&t!("cli.init.base_branch"), "--base-branch")?;
        Text::new(&t!("cli.init.base_branch"))
            .with_default("main")
            .prompt()?
    };

    let release_branch = if let Some(release_branch) = &init.release_branch {
        release_branch.clone()
    } else {
        require_interactive(&t!("cli.init.release_branch"), "--release-branch")?;
        Text::new(&t!("cli.init.release_branch"))
            .with_default("release")
            .prompt()?
    };

    let github_actions = if init.github_actions {
        true
    } else if init.no_github_actions {
        false
    } else {
        require_interactive(
            &t!("cli.init.github_actions"),
            "--github-actions or --no-github-actions",
        )?;
        Confirm::new(&t!("cli.init.github_actions"))
            .with_default(true)
            .prompt()?
    };

    let workflows = if github_actions {
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
            workflows,
        },
    )?;
    let report = service.apply_init(&plan)?;
    terminal.summary(
        StepOutcome::Success,
        &t!("cli.init.complete", files = report.files.len()),
    );

    Ok(())
}
