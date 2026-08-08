use clap::Parser;
use rust_i18n::t;
use semifold_engine::{
    AppError, ExecutionMode, Project, PublishOptions, PublishReport, PublishWorkflowOutput,
    SemifoldService, SystemDependencies, WorkflowExecutionMode,
};

use crate::cli::{
    repository_context,
    workflow_output::{GithubOutputWriter, PUBLISH_OUTPUT_KEY},
};

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
    let mut plan = service
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
    let mode = if dry_run {
        ExecutionMode::DryRun
    } else {
        ExecutionMode::Apply
    };
    let workflow_mode = if dry_run {
        WorkflowExecutionMode::DryRun
    } else {
        WorkflowExecutionMode::Apply
    };
    let result = service.publish(&mut plan, mode).await;
    let writer = GithubOutputWriter::from_environment();

    let report = match result {
        Ok(report) => {
            let output = PublishWorkflowOutput::from_plan_and_report(&plan, &report, workflow_mode);
            writer.write(PUBLISH_OUTPUT_KEY, &output).map_err(|error| {
                anyhow::anyhow!(t!("cli.publish.workflow_output_failed", error = error))
            })?;
            report
        }
        Err(error @ AppError::PublishExecution(_)) => {
            if let AppError::PublishExecution(execution) = &error {
                let output = PublishWorkflowOutput::from_plan_and_report(
                    &plan,
                    &execution.report,
                    workflow_mode,
                );
                if let Err(output_error) = writer.write(PUBLISH_OUTPUT_KEY, &output) {
                    log::warn!(
                        "{}",
                        t!("cli.publish.workflow_output_warning", error = output_error)
                    );
                }
            }
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    };

    Ok(report)
}

pub(crate) async fn run(opts: &Publish, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    SemifoldService::new(SystemDependencies)
        .ensure_clean_worktree(project, opts.allow_dirty)
        .map_err(super::version::render_worktree_error)?;

    let _report = publish(project, dry_run, opts.github_release).await?;

    Ok(())
}
