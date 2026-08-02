use clap::Parser;
use colored::Colorize;
use rust_i18n::t;
use semifold_core::{ChangesetId, RepositoryContext};
use semifold_engine::{
    AppError, ApplyReport, ExecutionMode, Project, ReleaseApplyError, ReleaseApplyPlan,
    ReleaseExecutionOptions, SemifoldService, SystemDependencies,
};

#[derive(Parser, Debug)]
pub(crate) struct Version {
    #[clap(long, help = t!("cli.version.flags.allow_dirty"))]
    allow_dirty: bool,
}

pub(crate) fn repository_context() -> Option<RepositoryContext> {
    let repository = std::env::var("GITHUB_REPOSITORY").ok()?;
    let (owner, name) = repository.split_once('/')?;
    let host =
        std::env::var("GITHUB_SERVER_URL").unwrap_or_else(|_| "https://github.com".to_string());
    Some(RepositoryContext {
        host: host.clone(),
        owner: owner.to_string(),
        name: name.to_string(),
        web_url: format!("{}/{owner}/{name}", host.trim_end_matches('/')),
        commit: None,
    })
}

pub(crate) fn is_git_repo_clean(project: &Project) -> anyhow::Result<bool> {
    let repository = git2::Repository::open(project.root.as_std_path())
        .map_err(|error| anyhow::anyhow!(t!("cli.version.git_open_failed", error = error)))?;
    let statuses = repository
        .statuses(None)
        .map_err(|error| anyhow::anyhow!(t!("cli.version.git_status_failed", error = error)))?;
    Ok(statuses.iter().all(|entry| {
        matches!(
            entry.status(),
            git2::Status::CURRENT | git2::Status::IGNORED
        )
    }))
}

pub(crate) async fn prepare_and_apply_release(
    project: &Project,
    release: semifold_core::ReleasePlan,
    dry_run: bool,
) -> anyhow::Result<ApplyReport> {
    let service = SemifoldService::new(SystemDependencies);
    let plan = service
        .prepare_release(
            project,
            release,
            &ReleaseExecutionOptions {
                collect_remote_metadata: !dry_run,
                repository: repository_context(),
            },
        )
        .await?;
    render_release_plan_activity(&plan, dry_run);
    service
        .apply_release(
            plan,
            if dry_run {
                ExecutionMode::DryRun
            } else {
                ExecutionMode::Apply
            },
        )
        .map_err(render_apply_error)
}

fn render_release_plan_activity(plan: &ReleaseApplyPlan, dry_run: bool) {
    for package in &plan.remote_metadata_failures {
        log::warn!(
            "{}",
            t!(
                "cli.version.changelog_metadata_degraded",
                package = package.as_str()
            )
        );
    }
    for planned in &plan.post_version_commands {
        let command = format!(
            "{} {}",
            planned.command.executable,
            planned.command.args.join(" ")
        );
        if dry_run && !planned.command.run_in_dry_run {
            log::warn!(
                "{}",
                t!(
                    "cli.version.skip_post_version",
                    command = command.magenta(),
                    package = planned.package.as_str().cyan()
                )
            );
        } else {
            log::info!(
                "{}",
                t!(
                    "cli.version.run_post_version",
                    command = command.magenta(),
                    package = planned.package.as_str().cyan()
                )
            );
        }
    }
}

fn render_apply_error(error: AppError) -> anyhow::Error {
    let AppError::ReleaseApply(ReleaseApplyError::PostVersion { report, failure }) = error else {
        return error.into();
    };
    let files = report.file_edits.as_ref().map_or_else(
        || "-".to_string(),
        |report| {
            report
                .applied
                .iter()
                .map(|path| path.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        },
    );
    let changesets = report
        .unconsumed_changesets
        .iter()
        .map(ChangesetId::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::anyhow!(t!(
        "cli.version.post_version_recovery",
        package = failure.package.as_str(),
        command = format!(
            "{} {}",
            failure.command.executable,
            failure.command.args.join(" ")
        ),
        error = failure.source,
        files = files,
        changesets = changesets
    ))
}

pub(crate) async fn run(opts: &Version, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    if !opts.allow_dirty && !is_git_repo_clean(project)? {
        return Err(anyhow::anyhow!(t!("cli.dirty_repo")));
    }

    let service = SemifoldService::new(SystemDependencies);
    let release = service.plan_release(project)?;
    if release.consumed_changesets().is_empty() {
        log::warn!("{}", t!("cli.version.empty_changesets"));
        return Ok(());
    }
    prepare_and_apply_release(project, release, dry_run).await?;
    Ok(())
}
