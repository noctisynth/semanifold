use std::env;

use anyhow::Context as _;
use clap::Parser;
use git2::{Cred, IndexAddOption, PushOptions, RemoteCallbacks, Repository};
use octocrab::{Octocrab, params};
use rust_i18n::t;

use semifold_resolver::{context::Context, resolver};

use crate::cli::{publish, version};

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
        return Err(anyhow::anyhow!("Not running in CI environment"));
    }

    let ref_name = env::var("GITHUB_REF_NAME").context("GITHUB_REF_NAME is not set")?;
    let github_repo = env::var("GITHUB_REPOSITORY").context("GITHUB_REPOSITORY is not set")?;

    log::debug!("GITHUB_REF_NAME: {}", &ref_name);

    let repo = Repository::open(ctx.repo_root.as_ref().unwrap())?;
    let mut git_config = repo.config()?;
    git_config.set_str("user.name", "github-actions")?;
    git_config.set_str("user.email", "github-actions@users.noreply.github.com")?;

    let (owner, repo_name) = github_repo.split_once('/').ok_or(anyhow::anyhow!(
        "GITHUB_REPOSITORY is not in the format owner/repo"
    ))?;

    let octocrab = Octocrab::builder()
        .personal_token(env::var("GITHUB_TOKEN")?)
        .build()?;

    let is_base_branch = ref_name == config.branches.base;
    if !is_base_branch {
        log::warn!("Not a push to base branch, skip versioning and publishing.");
        return Ok(());
    }

    let changesets = resolver::get_changesets(ctx)?;
    if changesets.is_empty() {
        log::info!("No changesets found, will publish the current version.");
        return publish::publish(config, false);
    }

    let changelogs_map = version::version(ctx, &changesets, false).await?;

    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    let base_branch = &config.branches.base;
    let release_branch = &config.branches.release;

    repo.branch(release_branch, &commit, true)?;
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

    force_push_release(&repo, &env::var("GITHUB_TOKEN")?, release_branch)?;

    let pulls = octocrab.pulls(owner, repo_name);
    let existing_prs = pulls
        .list()
        .state(params::State::Open)
        .head(release_branch)
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
        log::info!("No existing PR found, create a new one.");
        pulls
            .create(pr_title, release_branch, base_branch)
            .body(pr_body)
            .send()
            .await?;
    } else {
        let pr = existing_prs.first().unwrap();
        log::info!("Existing PR #{} found, skip creating a new one.", pr.number);
        pulls
            .update(pr.number)
            .title(pr_title)
            .body(pr_body)
            .send()
            .await?;
    }

    Ok(())
}
