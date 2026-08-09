use clap::Parser;
use colored::Colorize;
use rust_i18n::t;
use semifold_engine::{
    AppError, ExecutionMode, Project, PublishOptions, PublishReport, PublishWorkflowOutput,
    SemifoldService, SystemDependencies, WorkflowExecutionMode,
    publish_plan::PublishSkipReason,
    publisher::{ForgeDisposition, PublishFailureStage, PublishStatus},
};

use crate::cli::{
    repository_context,
    terminal::{StepOutcome, Terminal},
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
    terminal: &Terminal,
) -> anyhow::Result<PublishReport> {
    let should_create_github_release = std::env::var("GITHUB_ACTIONS").is_ok() && github_release;
    let repository = repository_context();
    if should_create_github_release && repository.is_none() {
        return Err(anyhow::anyhow!(t!("cli.publish.repo_info_missing")));
    }
    let service = SemifoldService::new(SystemDependencies);
    let planning = terminal.progress(t!("cli.publish.planning").into_owned());
    let plan_result = service
        .plan_publish(
            project,
            &PublishOptions {
                create_forge_release: should_create_github_release,
                repository,
            },
        )
        .await;
    let mut plan = match plan_result {
        Ok(plan) => plan,
        Err(error) => {
            planning.finish(
                StepOutcome::Failed,
                t!("cli.publish.planning_failed").into_owned(),
            );
            return Err(anyhow::anyhow!(t!(
                "cli.publish.plan_failed",
                error = error
            )));
        }
    };
    planning.finish(
        StepOutcome::Success,
        t!("cli.publish.planned", packages = plan.packages.len()).into_owned(),
    );
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
    let executing = terminal.progress(if dry_run {
        t!("cli.publish.simulating").into_owned()
    } else {
        t!("cli.publish.executing").into_owned()
    });
    let result = service.publish(&mut plan, mode).await;
    let writer = GithubOutputWriter::from_environment();

    let report = match result {
        Ok(report) => {
            executing.finish(
                StepOutcome::Success,
                if dry_run {
                    t!("cli.publish.simulated").into_owned()
                } else {
                    t!("cli.publish.executed").into_owned()
                },
            );
            render_publish_report(terminal, &plan, &report, dry_run);
            let output = PublishWorkflowOutput::from_plan_and_report(&plan, &report, workflow_mode);
            writer.write(PUBLISH_OUTPUT_KEY, &output).map_err(|error| {
                anyhow::anyhow!(t!("cli.publish.workflow_output_failed", error = error))
            })?;
            report
        }
        Err(error @ AppError::PublishExecution(_)) => {
            if let AppError::PublishExecution(execution) = &error {
                executing.finish(
                    StepOutcome::Failed,
                    t!("cli.publish.execution_failed").into_owned(),
                );
                render_publish_report(terminal, &plan, &execution.report, dry_run);
                let recovery = recovery_action(&execution.report);
                terminal.recovery(&t!("cli.publish.recovery_heading"), &recovery);
                let output = PublishWorkflowOutput::from_plan_and_report(
                    &plan,
                    &execution.report,
                    workflow_mode,
                );
                if let Err(output_error) = writer.write(PUBLISH_OUTPUT_KEY, &output) {
                    terminal.warning(&t!(
                        "cli.publish.workflow_output_warning",
                        error = output_error
                    ));
                }
            }
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    };

    Ok(report)
}

pub(crate) async fn run(opts: &Publish, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    let terminal = Terminal::detect();
    terminal.heading(&t!("cli.publish.heading"));
    if dry_run {
        terminal.dry_run(&t!("cli.common.dry_run_banner"));
    }
    SemifoldService::new(SystemDependencies)
        .ensure_clean_worktree(project, opts.allow_dirty)
        .map_err(super::version::render_worktree_error)?;

    let _report = publish(project, dry_run, opts.github_release, &terminal).await?;

    Ok(())
}

fn render_publish_report(
    terminal: &Terminal,
    plan: &semifold_engine::PublishPlan,
    report: &PublishReport,
    dry_run: bool,
) {
    terminal.blank();
    terminal.section(&t!("cli.publish.result"));
    terminal.line(format!(
        "  {} {} {} {}",
        Terminal::cell(t!("cli.publish.column_package"), 24),
        Terminal::cell(t!("cli.publish.column_version"), 16),
        Terminal::cell(t!("cli.publish.column_status"), 14),
        t!("cli.publish.column_detail")
    ));
    for (planned, package) in plan.packages.iter().zip(&report.packages) {
        let (status, detail) = match package.status {
            PublishStatus::Succeeded => (
                t!("cli.publish.status_succeeded"),
                forge_detail(package.forge),
            ),
            PublishStatus::Skipped(reason) => (
                t!("cli.publish.status_skipped"),
                skipped_detail(reason, package.forge),
            ),
            PublishStatus::Failed(stage) => {
                (t!("cli.publish.status_failed"), failure_detail(stage))
            }
            PublishStatus::NotStarted => (
                t!("cli.publish.status_not_started"),
                t!("cli.publish.detail_not_started"),
            ),
        };
        let status = Terminal::cell(status, 14);
        let detail = detail.into_owned();
        let (status, detail) = match package.status {
            PublishStatus::Succeeded => (status.green().bold(), detail.green()),
            PublishStatus::Skipped(_) => (status.yellow().bold(), detail.yellow()),
            PublishStatus::Failed(_) => (status.red().bold(), detail.red()),
            PublishStatus::NotStarted => (status.dimmed(), detail.dimmed()),
        };
        terminal.line(format!(
            "  {} {} {} {}",
            Terminal::cell(package.package.as_str(), 24).cyan().bold(),
            Terminal::cell(planned.context.package.version.to_string(), 16).yellow(),
            status,
            detail
        ));
    }
    let succeeded = report
        .packages
        .iter()
        .filter(|package| package.status == PublishStatus::Succeeded)
        .count();
    let skipped = report
        .packages
        .iter()
        .filter(|package| matches!(package.status, PublishStatus::Skipped(_)))
        .count();
    let failed = report
        .packages
        .iter()
        .filter(|package| matches!(package.status, PublishStatus::Failed(_)))
        .count();
    let not_started = report
        .packages
        .iter()
        .filter(|package| package.status == PublishStatus::NotStarted)
        .count();
    terminal.blank();
    terminal.fact(&t!("cli.publish.summary_succeeded"), succeeded);
    terminal.fact(&t!("cli.publish.summary_skipped"), skipped);
    terminal.fact(&t!("cli.publish.summary_failed"), failed);
    terminal.fact(&t!("cli.publish.summary_not_started"), not_started);
    terminal.blank();
    if failed == 0 {
        terminal.summary(
            StepOutcome::Success,
            &if dry_run {
                t!("cli.publish.dry_run_complete").into_owned()
            } else {
                t!("cli.publish.complete").into_owned()
            },
        );
    } else {
        terminal.summary(StepOutcome::Failed, &t!("cli.publish.stopped"));
    }
}

fn skip_detail(reason: PublishSkipReason) -> std::borrow::Cow<'static, str> {
    match reason {
        PublishSkipReason::Private => t!("cli.publish.detail_private"),
        PublishSkipReason::MissingChangelog => t!("cli.publish.detail_missing_changelog"),
        PublishSkipReason::RegistryVersionExists => t!("cli.publish.detail_already_exists"),
    }
}

fn skipped_detail(
    reason: PublishSkipReason,
    forge: ForgeDisposition,
) -> std::borrow::Cow<'static, str> {
    if reason == PublishSkipReason::RegistryVersionExists {
        return match forge {
            ForgeDisposition::SkippedDryRun => {
                t!("cli.publish.detail_registry_exists_forge_dry_run")
            }
            ForgeDisposition::Created => {
                t!("cli.publish.detail_registry_exists_forge_created")
            }
            ForgeDisposition::AlreadyExists => {
                t!("cli.publish.detail_registry_and_forge_exist")
            }
            ForgeDisposition::NotRequested => skip_detail(reason),
        };
    }
    skip_detail(reason)
}

fn failure_detail(stage: PublishFailureStage) -> std::borrow::Cow<'static, str> {
    match stage {
        PublishFailureStage::Preflight => t!("cli.publish.detail_preflight_failed"),
        PublishFailureStage::Command(_) => t!("cli.publish.detail_command_failed"),
        PublishFailureStage::ForgeRelease => t!("cli.publish.detail_forge_failed"),
        PublishFailureStage::AssetUpload => t!("cli.publish.detail_asset_failed"),
    }
}

fn forge_detail(disposition: ForgeDisposition) -> std::borrow::Cow<'static, str> {
    match disposition {
        ForgeDisposition::NotRequested => t!("cli.publish.detail_registry_complete"),
        ForgeDisposition::SkippedDryRun => t!("cli.publish.detail_forge_dry_run"),
        ForgeDisposition::Created => t!("cli.publish.detail_forge_created"),
        ForgeDisposition::AlreadyExists => t!("cli.publish.detail_forge_exists"),
    }
}

fn recovery_action(report: &PublishReport) -> std::borrow::Cow<'static, str> {
    if report.packages.iter().any(|package| {
        package.status == PublishStatus::Failed(PublishFailureStage::AssetUpload)
            && package.forge == ForgeDisposition::Created
    }) {
        t!("cli.publish.recovery_asset_failure")
    } else {
        t!("cli.publish.recovery_action")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_version_skip_detail_includes_the_forge_disposition() {
        assert_eq!(
            skipped_detail(
                PublishSkipReason::RegistryVersionExists,
                ForgeDisposition::Created,
            ),
            t!("cli.publish.detail_registry_exists_forge_created")
        );
        assert_eq!(
            skipped_detail(
                PublishSkipReason::RegistryVersionExists,
                ForgeDisposition::AlreadyExists,
            ),
            t!("cli.publish.detail_registry_and_forge_exist")
        );
        assert_eq!(
            skipped_detail(
                PublishSkipReason::RegistryVersionExists,
                ForgeDisposition::SkippedDryRun,
            ),
            t!("cli.publish.detail_registry_exists_forge_dry_run")
        );
    }

    #[test]
    fn registry_version_skip_without_forge_keeps_the_existing_detail() {
        assert_eq!(
            skipped_detail(
                PublishSkipReason::RegistryVersionExists,
                ForgeDisposition::NotRequested,
            ),
            t!("cli.publish.detail_already_exists")
        );
    }

    #[test]
    fn asset_failure_after_release_creation_requires_a_new_version() {
        let report = PublishReport {
            packages: vec![semifold_engine::publisher::PackagePublishReport {
                package: semifold_core::PackageId::new("core"),
                status: PublishStatus::Failed(PublishFailureStage::AssetUpload),
                commands: Vec::new(),
                forge: ForgeDisposition::Created,
                error: Some("upload failed".to_string()),
            }],
        };

        assert_eq!(
            recovery_action(&report),
            t!("cli.publish.recovery_asset_failure")
        );
    }
}
