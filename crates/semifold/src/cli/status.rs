use std::{collections::HashMap, env};

use anyhow::Context as _;
use clap::Parser;
use colored::Colorize;
use octocrab::Octocrab;
use rust_i18n::t;
use semifold_resolver::{changeset::BumpLevel, context::Context, resolver, utils};
use serde::{Deserialize, Serialize};

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
pub(crate) struct PullRequest {
    pub number: u64,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct GitHubEvent {
    pub repository: Repository,
    pub pull_request: PullRequest,
}

#[derive(Parser, Debug)]
pub(crate) struct Status {
    /// Create GitHub pull request comments, only available for pull requests
    #[arg(short, long, default_value_t = true)]
    pub comment: bool,
}

pub(crate) async fn run(status: &Status, ctx: &Context) -> anyhow::Result<()> {
    let Context {
        config: Some(config),
        ..
    } = ctx
    else {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    let is_ci = ctx.is_ci();
    log::debug!("GitHub CI environment: {}", is_ci);
    let root = ctx.repo_root.clone().unwrap_or(std::env::current_dir()?);

    let changesets = resolver::get_changesets(ctx)?;
    let name_width = config.packages.keys().map(|s| s.len()).max().unwrap_or(0) + 1;

    println!(
        "{}\n",
        t!(
            "cli.status.changesets",
            count = changesets.len().to_string().bold()
        )
    );

    let mut bump_map = HashMap::new();
    for (package_name, package_config) in &config.packages {
        let level = utils::get_bump_level(&changesets, package_name);
        if matches!(level, BumpLevel::Unchanged) {
            continue;
        }

        let mut resolver = package_config.resolver.get_resolver();
        let resolved_package = resolver.resolve(&root, package_config)?;
        let bumped_version = utils::bump_version(&resolved_package.version, level)?;

        bump_map.insert(
            package_name,
            (
                level,
                resolved_package.version.clone(),
                bumped_version.clone(),
            ),
        );
    }

    if bump_map.is_empty() {
        println!("{}", t!("cli.status.no_packages"));
    } else {
        println!("{}", t!("cli.status.packages"));
        for (package_name, (_, resolved_version, bumped_version)) in &bump_map {
            println!(
                "{:name_width$} {} → {}",
                package_name.cyan().bold(),
                resolved_version.yellow(),
                bumped_version.to_string().green()
            );
        }
    }

    if !is_ci {
        return Ok(());
    }

    let path = env::var("GITHUB_EVENT_PATH").context("no GITHUB_EVENT_PATH")?;
    let event_data = tokio::fs::read_to_string(&path).await?;

    log::debug!("GITHUB_EVENT_PATH: {}", &path);
    log::debug!("GITHUB_EVENT_PATH data: {}", &event_data);

    let event: GitHubEvent = serde_json::from_str(&event_data)?;

    // let (owner, repo_name) = github_repo.split_once('/').ok_or(anyhow::anyhow!(
    //     "GITHUB_REPOSITORY is not in the format owner/repo"
    // ))?;
    let owner = &event.repository.owner.login;
    let repo_name = &event.repository.name;
    // let pr_number = ref_name
    //     .split_once('/')
    //     .ok_or_else(|| anyhow::anyhow!("GITHUB_REF_NAME is not in the format <pr_number>/merge"))?
    //     .0
    //     .parse::<u64>()?;
    let pr_number = event.pull_request.number;

    let octocrab = Octocrab::builder()
        .personal_token(env::var("GITHUB_TOKEN")?)
        .build()?;

    // let is_pull_request = base_ref == config.branches.base && head_ref != config.branches.base;
    // log::debug!("is_pull_request: {}", is_pull_request);
    if status.comment {
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
        let comment_body = format!(
            "## Workspace change through: {}\n\n\
            {} changesets found\n\n\
            <details>\n\
            <summary>Planned changes to release</summary>\n\n\
            | Package | Bump Level | Current Version | Next Version |\n\
            | ------- | ---------- | --------------- | ------------ |\n\
            {}\n\
            </details>",
            &last_commit.sha,
            changesets.len(),
            &markdown_table,
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
