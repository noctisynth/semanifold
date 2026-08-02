use clap::Parser;
use rust_i18n::t;
use semifold_engine::{
    ExecutionMode, Project, PublishOptions, PublishReport, SemifoldService, SystemDependencies,
};

use crate::cli::{is_git_repo_clean, repository_context};

#[derive(Debug, Parser)]
pub(crate) struct Publish {
    #[clap(short = 'r', long, default_value_t = true, help = t!("cli.publish.flags.github_release"))]
    github_release: bool,
    #[clap(short = 'd', long, default_value_t = false, help = t!("cli.publish.flags.allow_dirty"))]
    allow_dirty: bool,
}

pub(crate) async fn publish(
    project: &Project,
    dry_run: bool,
    github_release: bool,
) -> anyhow::Result<PublishReport> {
    log::debug!(
        "Packages to publish: {:?}",
        project.config.packages.keys().collect::<Vec<_>>()
    );

    let should_create_github_release = std::env::var("GITHUB_ACTIONS").is_ok() && github_release;
    let repository = repository_context();
    if should_create_github_release && repository.is_none() {
        return Err(anyhow::anyhow!(t!("cli.publish.repo_info_missing")));
    }
    let service = SemifoldService::new(SystemDependencies);
    let plan = service
        .plan_publish(
            project,
            &PublishOptions {
                create_forge_release: should_create_github_release,
                repository,
            },
        )
        .await
        .map_err(|error| anyhow::anyhow!(t!("cli.publish.plan_failed", error = error)))?;
    log::debug!("Packages to publish: {:?}", plan.packages);
    let report = service
        .publish(
            plan,
            if dry_run {
                ExecutionMode::DryRun
            } else {
                ExecutionMode::Apply
            },
        )
        .await?;

    Ok(report)
}

pub(crate) async fn run(opts: &Publish, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    if !opts.allow_dirty && !is_git_repo_clean(project)? {
        return Err(anyhow::anyhow!(t!("cli.dirty_repo")));
    }

    let _report = publish(project, dry_run, opts.github_release).await?;

    Ok(())
}
