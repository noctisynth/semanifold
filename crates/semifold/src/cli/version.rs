use clap::Parser;
use colored::Colorize;
use rust_i18n::t;
use semifold_core::ChangesetId;
use semifold_engine::{
    AppError, ApplyReport, ExecutionMode, Project, ReleaseApplyError, ReleaseApplyPlan,
    ReleaseExecutionOptions, SemifoldService, SystemDependencies, VersionWorkflowOutput,
    WorkflowExecutionMode,
};

use crate::cli::{
    repository_context,
    workflow_output::{GithubOutputWriter, VERSION_OUTPUT_KEY},
};

#[derive(Parser, Debug)]
pub(crate) struct Version {
    #[clap(long, help = t!("cli.version.flags.allow_dirty"))]
    allow_dirty: bool,
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
        .await
        .map_err(|error| anyhow::anyhow!(t!("cli.version.prepare_failed", error = error)))?;
    render_release_plan_activity(&plan, dry_run);
    let output = VersionWorkflowOutput::from_release(
        &plan.release_context,
        plan.release_branch.clone(),
        if dry_run {
            WorkflowExecutionMode::DryRun
        } else {
            WorkflowExecutionMode::Apply
        },
    );
    let report = service
        .apply_release(
            plan,
            if dry_run {
                ExecutionMode::DryRun
            } else {
                ExecutionMode::Apply
            },
        )
        .map_err(render_apply_error)?;
    GithubOutputWriter::from_environment()
        .write(VERSION_OUTPUT_KEY, &output)
        .map_err(|error| {
            anyhow::anyhow!(t!("cli.version.workflow_output_failed", error = error))
        })?;
    Ok(report)
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
    let (report, failure) = match error {
        AppError::ReleaseApply(error) => match *error {
            ReleaseApplyError::PostVersion { report, failure } => (report, failure),
            error => return AppError::ReleaseApply(Box::new(error)).into(),
        },
        error => return error.into(),
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
    let service = SemifoldService::new(SystemDependencies);
    service
        .ensure_clean_worktree(project, opts.allow_dirty)
        .map_err(render_worktree_error)?;
    let release = service.plan_release(project)?;
    if release.consumed_changesets().is_empty() {
        log::warn!("{}", t!("cli.version.empty_changesets"));
        return Ok(());
    }
    prepare_and_apply_release(project, release, dry_run).await?;
    Ok(())
}

pub(crate) fn render_worktree_error(error: AppError) -> anyhow::Error {
    match error {
        AppError::DirtyWorktree => anyhow::anyhow!(t!("cli.dirty_repo")),
        AppError::GitOpen(error) => {
            anyhow::anyhow!(t!("cli.version.git_open_failed", error = error))
        }
        AppError::GitStatus(error) => {
            anyhow::anyhow!(t!("cli.version.git_status_failed", error = error))
        }
        error => error.into(),
    }
}
