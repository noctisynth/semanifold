use std::{collections::BTreeMap, env};

use anyhow::Context as _;
use clap::Parser;
use colored::Colorize;
use octocrab::Octocrab;
use rust_i18n::t;
use semifold_core::PlanWarning;
use semifold_resolver::{context::Context, resolver};
use serde::{Deserialize, Serialize};

use crate::release::plan_release;

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

pub(crate) async fn run(status: &Status, ctx: &Context) -> anyhow::Result<()> {
    if !ctx.is_initialized() {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    let is_ci = ctx.is_ci();
    log::debug!("GitHub CI environment: {}", is_ci);

    let root = ctx.repo_root.clone().unwrap_or(std::env::current_dir()?);
    let config = ctx.config.as_ref().unwrap();

    let changesets = resolver::get_changesets(ctx)?;
    let plan = plan_release(&root, config, &changesets)
        .map_err(|error| anyhow::anyhow!(t!("cli.status.plan_failed", error = error)))?;
    let name_width = plan
        .packages()
        .iter()
        .map(|package| package.id.as_str().len())
        .max()
        .unwrap_or(0)
        + 1;

    println!(
        "{}\n",
        t!(
            "cli.status.changesets",
            count = plan.consumed_changesets().len().to_string().bold()
        )
    );

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
        println!("{}", t!("cli.status.no_packages"));
    } else {
        println!("{}", t!("cli.status.packages"));
        for (package_name, (_, resolved_version, bumped_version)) in &bump_map {
            println!(
                "{:name_width$} {} → {}",
                package_name.cyan().bold(),
                resolved_version.to_string().yellow(),
                bumped_version.to_string().green()
            );
        }
    }

    if !warnings.is_empty() {
        println!("\n{}", t!("cli.status.pre_release_warning_header").yellow());
    }
    for warning in warnings.iter() {
        println!("{}", warning.yellow());
    }

    if !is_ci {
        return Ok(());
    }

    let path = env::var("GITHUB_EVENT_PATH").context("no GITHUB_EVENT_PATH")?;
    let event_data = std::fs::read_to_string(&path)?;

    log::debug!("GITHUB_EVENT_PATH: {}", path);
    log::debug!("GITHUB_EVENT_PATH data: {}", event_data);

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
                log::warn!("Failed to create comment: {:?}", e);
            };
        } else if let Err(e) = octocrab
            .issues(owner, repo_name)
            .create_comment(pr_number, comment_body)
            .await
        {
            log::warn!("Failed to create comment: {:?}", e);
        };
    }

    Ok(())
}
