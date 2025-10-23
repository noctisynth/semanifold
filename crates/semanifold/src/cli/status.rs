use std::env;

use clap::Parser;
use colored::Colorize;
use octocrab::Octocrab;
use rust_i18n::t;
use semanifold_resolver::{context::Context, resolver, utils};

#[derive(Parser, Debug)]
pub(crate) struct Status {
    /// Create GitHub pull request comments, only available for pull requests
    #[arg(short, long, default_value_t = true)]
    pub comment: bool,
}

pub(crate) async fn run(status: &Status, ctx: &Context) -> anyhow::Result<()> {
    let Context {
        config: Some(config),
        changeset_root: Some(changeset_root),
        ..
    } = ctx
    else {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    let is_ci = ctx.is_ci();
    log::debug!("GitHub CI environment: {}", is_ci);

    let changesets = resolver::get_changesets(changeset_root)?;
    let name_width = config.packages.keys().map(|s| s.len()).max().unwrap_or(0) + 1;

    for (package_name, package_config) in &config.packages {
        let level = utils::get_bump_level(&changesets, package_name);
        let mut resolver = package_config.resolver.get_resolver();
        let resolved_package = resolver.resolve(package_config)?;

        println!(
            "{:name_width$} {} → {}",
            package_name.cyan(),
            resolved_package.version.yellow(),
            utils::bump_version(&resolved_package.version, level)?
                .to_string()
                .green()
        );
    }

    if !is_ci {
        return Ok(());
    }

    let base_ref = env::var("GITHUB_BASE_REF").unwrap_or_default();
    let head_ref = env::var("GITHUB_HEAD_REF").unwrap_or_default();
    let ref_name = env::var("GITHUB_REF_NAME").unwrap_or_default();
    log::debug!("GITHUB_REF_NAME: {}", &ref_name);
    log::debug!("GITHUB_HEAD_REF: {}", &head_ref);
    log::debug!("GITHUB_BASE_REF: {}", &base_ref);
    let github_repo = env::var("GITHUB_REPOSITORY")?;
    log::debug!("GITHUB_REPOSITORY: {}", &github_repo);

    let (owner, repo_name) = github_repo
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("GITHUB_REPOSITORY is not in the format owner/repo"))?;

    let octocrab = Octocrab::builder()
        .personal_token(env::var("GITHUB_TOKEN")?)
        .build()?;

    let is_pull_request = base_ref == config.branches.base && head_ref != config.branches.base;
    log::debug!("is_pull_request: {}", is_pull_request);
    if status.comment && is_pull_request {
        let pr = octocrab.pulls(owner, repo_name);
        let comments = pr.list_comments(None).send().await?;
        log::debug!("comments: {:?}", comments);
        for comment in comments {
            log::debug!("comment: {:?}", comment);
        }
    }

    Ok(())
}
