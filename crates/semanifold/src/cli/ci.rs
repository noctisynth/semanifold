use std::env;

use clap::Parser;
use git2::{IndexAddOption, Repository};
use octocrab::Octocrab;
use rust_i18n::t;
use semanifold_resolver::{context::Context, resolver};

use crate::cli::{publish, version};

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

    let repo = Repository::open(changeset_root.parent().unwrap())?;
    let base_ref = env::var("GITHUB_BASE_REF").unwrap_or_default();
    let github_repo = env::var("GITHUB_REPOSITORY")?;
    let (owner, repo_name) = github_repo
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("GITHUB_REPOSITORY is not in the format owner/repo"))?;

    let octocrab = Octocrab::builder()
        .personal_token(env::var("GITHUB_TOKEN")?)
        .build()?;

    log::debug!("GITHUB_REF_NAME: {}", &base_ref);

    let is_pull_request = base_ref == config.branches.base;
    if is_pull_request {
        let comments = octocrab
            .pulls(owner, repo_name)
            .list_comments(None)
            .send()
            .await?;
        log::debug!("comments: {:?}", comments);
    }

    let publish = resolver::get_changesets(changeset_root)?.is_empty();
    if publish {
        // TODO: check postpublish

        return publish::publish(config, false);
    }

    version::version(config, changeset_root, false)?;

    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    let release_branch = &config.branches.base;
    match repo.find_branch(release_branch, git2::BranchType::Local) {
        Ok(_) => {
            repo.set_head(&format!("refs/heads/{}", release_branch))?;
        }
        Err(_) => {
            repo.branch(release_branch, &commit, false)?;
            repo.set_head(&format!("refs/heads/{}", release_branch))?;
        }
    }

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
            "main",
        )
        .body("Automated update via GitHub Actions")
        .send()
        .await?;

    Ok(())
}
