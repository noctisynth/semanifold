use std::{
    collections::{BTreeMap, BTreeSet},
    env,
};

use anyhow::Context as _;
use clap::Parser;
use colored::Colorize;
use octocrab::Octocrab;
use rust_i18n::t;
use semifold_core::{PlanWarning, ReleaseReason};
use semifold_engine::{Project, SemifoldService, SystemDependencies};
use serde::{Deserialize, Serialize};

use crate::cli::terminal::{StepOutcome, Terminal};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct RepoOwner {
    pub login: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Repository {
    pub name: String,
    pub owner: RepoOwner,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Branch {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub label: String,
    pub repo: Repository,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct PullRequest {
    pub number: u64,
    pub head: Branch,
    pub base: Branch,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct GitHubEvent {
    pub repository: Repository,
    pub pull_request: PullRequest,
}

#[derive(Parser, Debug)]
pub(crate) struct Status {
    #[arg(short, long, default_value_t = true, help = t!("cli.status.flags.comment"))]
    pub comment: bool,
}

pub(crate) async fn run(status: &Status, project: &Project) -> anyhow::Result<()> {
    let terminal = Terminal::detect();
    terminal.heading(&t!("cli.status.heading"));
    let is_ci = env::var("GITHUB_ACTIONS").is_ok();
    let config = &project.config;
    log::debug!("GitHub CI environment: {}", is_ci);

    let progress = terminal.progress(t!("cli.status.planning").into_owned());
    let plan = match SemifoldService::new(SystemDependencies).plan_release(project) {
        Ok(plan) => plan,
        Err(error) => {
            progress.finish(
                StepOutcome::Failed,
                t!("cli.status.planning_failed").into_owned(),
            );
            return Err(anyhow::anyhow!(t!("cli.status.plan_failed", error = error)));
        }
    };
    progress.finish(
        StepOutcome::Success,
        t!("cli.status.planned", count = plan.packages().len()).into_owned(),
    );
    let name_width = plan
        .packages()
        .iter()
        .map(|package| package.id.as_str().len())
        .max()
        .unwrap_or(0)
        + 1;

    terminal.fact(
        &t!("cli.status.fact_changesets"),
        plan.consumed_changesets().len(),
    );
    terminal.fact(&t!("cli.status.fact_packages"), plan.packages().len());
    terminal.fact(
        &t!("cli.status.fact_fingerprint"),
        semifold_core::ReleaseContext::from_plan(&plan)
            .plan
            .fingerprint,
    );
    terminal.blank();

    let bump_map = plan
        .packages()
        .iter()
        .map(|package| {
            (
                package.id.as_str(),
                (
                    package.bump,
                    &package.current_version,
                    &package.next_version,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let warnings = plan
        .warnings()
        .iter()
        .map(|warning| match warning {
            PlanWarning::NonPatchBumpOnPrerelease { package, .. } => t!(
                "cli.status.pre_release_warning",
                package = package.as_str().cyan()
            ),
        })
        .collect::<Vec<_>>();

    if bump_map.is_empty() {
        terminal.summary(StepOutcome::Skipped, &t!("cli.status.no_packages"));
    } else {
        terminal.section(&t!("cli.status.packages"));
        terminal.line(format!(
            "  {} {} {} {} {}",
            Terminal::cell(t!("cli.status.column_package"), name_width),
            Terminal::cell(t!("cli.status.column_current"), 16),
            Terminal::cell(t!("cli.status.column_next"), 16),
            Terminal::cell(t!("cli.status.column_bump"), 8),
            t!("cli.status.column_reason")
        ));
        for package in plan.packages() {
            let reason = package
                .reasons
                .iter()
                .map(|reason| match reason {
                    ReleaseReason::Changeset { .. } => t!("cli.status.reason_changeset"),
                    ReleaseReason::DependencyPropagation { .. } => {
                        t!("cli.status.reason_dependency")
                    }
                    ReleaseReason::SharedVersionPropagation { .. } => {
                        t!("cli.status.reason_shared_version")
                    }
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ");
            terminal.line(format!(
                "  {} {} {} {} {}",
                Terminal::cell(package.id.as_str(), name_width)
                    .cyan()
                    .bold(),
                Terminal::cell(package.current_version.to_string(), 16).yellow(),
                Terminal::cell(package.next_version.to_string(), 16).green(),
                Terminal::cell(format!("{:?}", package.bump).to_lowercase(), 8),
                reason
            ));
        }
        terminal.blank();
        terminal.summary(StepOutcome::Success, &t!("cli.status.complete"));
    }

    if !warnings.is_empty() {
        terminal.warning(&t!("cli.status.pre_release_warning_header"));
    }
    for warning in warnings.iter() {
        terminal.warning(warning);
    }

    if !is_ci {
        return Ok(());
    }

    let path = env::var("GITHUB_EVENT_PATH").context("no GITHUB_EVENT_PATH")?;
    let event_data = std::fs::read_to_string(&path)?;

    log::debug!("GITHUB_EVENT_PATH: {}", path);
    log::debug!("Loaded GitHub event payload ({} bytes)", event_data.len());

    let event: GitHubEvent = serde_json::from_str(&event_data)?;

    let owner = &event.repository.owner.login;
    let head_owner = &event.pull_request.head.repo.owner.login;
    let repo_name = &event.repository.name;
    let pr_number = event.pull_request.number;
    let head_ref = event.pull_request.head.ref_name;
    let base_ref = event.pull_request.base.ref_name;

    log::debug!("owner: {}", owner);
    log::debug!("repo_name: {}", repo_name);
    log::debug!("pr_number: {}", pr_number);
    log::debug!("head_ref: {}", head_ref);
    log::debug!("base_ref: {}", base_ref);

    let octocrab = Octocrab::builder()
        .personal_token(env::var("GITHUB_TOKEN")?)
        .build()?;

    let is_matched = base_ref == config.branches.base
        && (head_ref != config.branches.base || head_owner != owner);
    if status.comment && is_matched {
        let issues = octocrab.issues(owner, repo_name);

        let comments = issues.list_comments(pr_number).send().await?.take_items();
        let commits = octocrab
            .pulls(owner, repo_name)
            .pr_commits(pr_number)
            .send()
            .await?;
        let last_commit = commits
            .into_iter()
            .last()
            .ok_or(anyhow::anyhow!("No commits found"))?;

        let existing = comments
            .iter()
            .find(|c| c.user.login == "github-actions[bot]");

        let markdown_table = bump_map
            .iter()
            .map(|(k, (l, v, b))| format!("| {} | {} | {} | {} |", k, l, v, b))
            .collect::<Vec<_>>()
            .join("\n");
        let warnings_section = if !warnings.is_empty() {
            let warnings_md = warnings
                .iter()
                .map(|w| format!("- {}", w))
                .collect::<Vec<_>>()
                .join("\n");
            format!("\n### Warnings\n\n{}", warnings_md)
        } else {
            String::new()
        };
        let comment_body = format!(
            "## Workspace change through: {}\n\n\
            {} changesets found\n\n\
            <details>\n\
            <summary>Planned changes to release</summary>\n\n\
            | Package | Bump Level | Current Version | Next Version |\n\
            | ------- | ---------- | --------------- | ------------ |\n\
            {}\n\
            </details>\n\
            {}",
            last_commit.sha,
            plan.consumed_changesets().len(),
            markdown_table,
            warnings_section,
        );

        if let Some(comment) = existing {
            if let Err(e) = octocrab
                .issues(owner, repo_name)
                .update_comment(comment.id, comment_body)
                .await
            {
                terminal.warning(&t!("cli.status.comment_failed", error = e));
            };
        } else if let Err(e) = octocrab
            .issues(owner, repo_name)
            .create_comment(pr_number, comment_body)
            .await
        {
            terminal.warning(&t!("cli.status.comment_failed", error = e));
        };
    }

    Ok(())
}
