use std::env;

use clap::Parser;
use git2::{IndexAddOption, Repository};
use octocrab::Octocrab;
use rust_i18n::t;
use semanifold_resolver::context::Context;

use crate::cli::version;

#[derive(Debug, Parser)]
pub struct CI;

pub(crate) async fn run(_ci: &CI, ctx: &Context) -> anyhow::Result<()> {
    let Context {
        config: Some(config),
        changeset_root: Some(changeset_root),
        ..
    } = ctx
    else {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    let base_ref = env::var("GITHUB_BASE_REF").unwrap_or_default();
    let head_ref = env::var("GITHUB_HEAD_REF").unwrap_or_default();
    let github_repo = env::var("GITHUB_REPOSITORY")?;

    log::debug!("GITHUB_HEAD_REF: {}", &head_ref);
    log::debug!("GITHUB_BASE_REF: {}", &base_ref);

    let repo = Repository::open(changeset_root.parent().unwrap())?;
    let (owner, repo_name) = github_repo.split_once('/').ok_or(anyhow::anyhow!(
        "GITHUB_REPOSITORY is not in the format owner/repo"
    ))?;

    let octocrab = Octocrab::builder()
        .personal_token(env::var("GITHUB_TOKEN")?)
        .build()?;

    let is_pull_request = base_ref == config.branches.base && head_ref != config.branches.base;
    if !is_pull_request {
        log::warn!("Not a pull request to base branch, skip versioning and publishing.");
        return Ok(());
    }

    version::version(config, changeset_root, false)?;

    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    let base_branch = &config.branches.base;
    let release_branch = &config.branches.release;
    repo.branch(release_branch, &commit, true)?;

    let mut co = git2::build::CheckoutBuilder::new();
    repo.set_head(&format!("refs/heads/{}", release_branch))?;
    repo.checkout_head(Some(co.force()))?;

    let mut index = repo.index()?;
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;

    let mut remote = repo.find_remote("origin")?;
    remote.push(&[&format!("refs/heads/{}", release_branch)], None)?;

    let _pr = octocrab
        .pulls(owner, repo_name)
        .create(
            format!("release: {}", release_branch),
            release_branch,
            base_branch,
        )
        .body(
            "# Releases\n\n\
            Changelogs is still under development.\n",
        )
        .send()
        .await?;

    Ok(())
}
