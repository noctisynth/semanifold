use std::env;

use anyhow::Context as _;
use clap::Parser;
use git2::{Cred, IndexAddOption, PushOptions, RemoteCallbacks, Repository};
use octocrab::{Octocrab, params};
use rust_i18n::t;

use semifold_core::ReleaseContext;
use semifold_resolver::{context::Context, resolver};

use crate::{
    cli::{publish, version},
    release::{plan_release, render_release_branch},
};

#[derive(Debug, Parser)]
pub struct CI;

fn build_callbacks(token: &str) -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();
    let token = token.to_string();

    callbacks.credentials(move |_url, username_from_url, _allowed_types| {
        if username_from_url.is_some() {
            Cred::userpass_plaintext(&token, "")
        } else {
            Cred::userpass_plaintext("x-access-token", &token)
        }
    });

    callbacks
}

fn force_push_release(repo: &Repository, token: &str, branch: &str) -> anyhow::Result<()> {
    let callbacks = build_callbacks(token);
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);

    let mut remote = repo.find_remote("origin").context("find remote origin")?;
    let ref_spec = format!("+refs/heads/{branch}:refs/heads/{branch}", branch = branch);
    remote.push(&[&ref_spec], Some(&mut push_opts))?;
    Ok(())
}

pub(crate) async fn run(_ci: &CI, ctx: &Context) -> anyhow::Result<()> {
    let Context {
        config: Some(config),
        ..
    } = ctx
    else {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    if !ctx.is_ci() {
        return Err(anyhow::anyhow!(t!("cli.ci.not_ci_environment")));
    }

    let ref_name = env::var("GITHUB_REF_NAME").context("GITHUB_REF_NAME is not set")?;
    let github_repo = env::var("GITHUB_REPOSITORY").context("GITHUB_REPOSITORY is not set")?;

    log::debug!("GITHUB_REF_NAME: {}", ref_name);

    let Some(repo) = ctx.git_repo.as_ref() else {
        return Err(anyhow::anyhow!(t!("cli.ci.git_repo_not_initialized")));
    };
    let mut git_config = repo.config()?;
    git_config.set_str("user.name", "github-actions[bot]")?;
    git_config.set_str("user.email", "github-actions[bot]@users.noreply.github.com")?;

    let (owner, repo_name) = github_repo
        .split_once('/')
        .ok_or(anyhow::anyhow!(t!("cli.ci.github_repo_invalid_format")))?;

    let github_token = env::var("GITHUB_TOKEN").context("GITHUB_TOKEN is not set")?;
    let octocrab = Octocrab::builder().personal_token(&*github_token).build()?;

    let is_base_branch = ref_name == config.branches.base;
    if !is_base_branch {
        log::warn!("{}", t!("cli.ci.not_base_branch"));
        return Ok(());
    }

    let changesets = resolver::get_changesets(ctx)?;
    if changesets.is_empty() {
        log::info!("{}", t!("cli.ci.no_changesets_publish"));
        return publish::publish(ctx, true).await;
    }

    let root = ctx
        .repo_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!(t!("cli.ci.git_repo_not_initialized")))?;
    let release_plan = plan_release(root, config, &changesets)?;
    let release_context = ReleaseContext::from_plan(&release_plan);
    let release_branch = render_release_branch(&config.branches.release, &release_context)
        .map_err(|error| anyhow::anyhow!(t!("cli.ci.release_branch_invalid", error = error)))?;

    let version::ApplyReport {
        changelogs: changelogs_map,
        file_edits,
        unconsumed_changesets: _,
    } = version::apply_version_plan(ctx, &changesets, release_plan).await?;
    let _applied_file_count = file_edits.as_ref().map_or(0, |report| report.applied.len());

    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    let base_branch = &config.branches.base;
    repo.branch(&release_branch, &commit, true)?;
    repo.set_head(&format!("refs/heads/{}", release_branch))?;
    repo.checkout_head(None)?;

    let mut index = repo.index()?;
    index.add_all(["."].iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;

    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let sig = repo.signature()?;
    let parent_commit = repo.head()?.peel_to_commit()?;
    let commit_message = "chore(release): bump versions";
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        commit_message,
        &tree,
        &[&parent_commit],
    )?;

    force_push_release(repo, &github_token, &release_branch)?;

    let head = format!("{}:{}", owner, release_branch);
    let pulls = octocrab.pulls(owner, repo_name);
    let existing_prs = pulls
        .list()
        .state(params::State::Open)
        .head(head)
        .base(base_branch)
        .send()
        .await?
        .take_items();

    let pr_title = "chore(release): bump versions";
    let pr_body = format!(
        "# Releases\n\n{}",
        changelogs_map
            .into_iter()
            .map(|(name, changelog)| { format!("## {name}\n\n{changelog}") })
            .collect::<Vec<_>>()
            .join("\n\n")
    );

    if existing_prs.is_empty() {
        log::info!("{}", t!("cli.ci.no_existing_pr"));
        pulls
            .create(pr_title, &release_branch, base_branch)
            .body(pr_body)
            .send()
            .await?;
    } else {
        let pr = existing_prs
            .first()
            .expect("non-empty pull request list must have a first entry");
        log::info!("{}", t!("cli.ci.existing_pr_found", number = pr.number));
        pulls
            .update(pr.number)
            .title(pr_title)
            .body(pr_body)
            .send()
            .await?;
    }

    Ok(())
}
