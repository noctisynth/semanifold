use anyhow::Context as _;
use clap::Parser;
use colored::Colorize;
use rust_i18n::t;

use semifold_changelog::read_latest_changelog;
use semifold_resolver::context::Context;

#[derive(Debug, Parser)]
pub(crate) struct Publish {
    /// Whether to create a GitHub release, only available when running in CI
    #[clap(short = 'r', long, default_value_t = true)]
    github_release: bool,
    /// Whether to publish the package
    #[clap(long)]
    dry_run: bool,
}

pub(crate) fn publish(ctx: &Context, dry_run: bool) -> anyhow::Result<()> {
    let config = ctx.config.as_ref().unwrap();
    let packages = config.packages.values().collect::<Vec<_>>();

    log::debug!("Packages to publish: {:?}", &packages);

    let root = ctx.repo_root.clone().unwrap_or(std::env::current_dir()?);
    for package in packages {
        let mut resolver = package.resolver.get_resolver();
        let resolved_package = resolver.resolve(&root, package)?;
        log::debug!("Resolved package: {}", &resolved_package.name);

        let resolver_config = config
            .resolver
            .get(&package.resolver)
            .ok_or(anyhow::anyhow!(
                "Config for resolver {} not found",
                &package.resolver
            ))?;
        log::debug!("Resolver config: {:?}", &resolver_config);
        resolver.publish(&resolved_package, resolver_config, dry_run)?;
    }

    Ok(())
}

pub(crate) async fn run(opts: &Publish, ctx: &Context) -> anyhow::Result<()> {
    if !ctx.is_initialized() {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    publish(ctx, opts.dry_run)?;

    if !ctx.is_ci() || !ctx.is_git_repo() || !opts.github_release {
        return Ok(());
    };

    let Some(repo_info) = &ctx.repo_info else {
        return Err(anyhow::anyhow!(t!("cli.github_token_not_set")));
    };
    let config = ctx.config.as_ref().unwrap();

    let octocrab = octocrab::Octocrab::builder()
        .personal_token(std::env::var("GITHUB_TOKEN").context("GITHUB_TOKEN is not set")?)
        .build()?;
    let releases = octocrab
        .repos(&repo_info.owner, &repo_info.repo_name)
        .releases()
        .list()
        .send()
        .await?
        .take_items();
    for (package_name, package_config) in &config.packages {
        let changelog_path = package_config.path.join("CHANGELOG.md");
        if !changelog_path.exists() {
            log::warn!(
                "Changelog file not found for package {}, skip create GitHub release",
                &package_name.cyan()
            );
            continue;
        }

        let changelog = read_latest_changelog(&changelog_path).await?;
        let tag_name = format!("{}-{}", package_name, changelog.version);

        log::debug!("Tag name: {}", &tag_name);
        log::debug!("Changelog for {}:\n\n{}", &package_name, &changelog.body);

        if releases.iter().any(|release| release.tag_name == tag_name) {
            log::warn!("Release {} already exists", &tag_name);
            continue;
        }

        octocrab
            .repos(&repo_info.owner, &repo_info.repo_name)
            .releases()
            .create(&tag_name)
            .body(&changelog.body)
            .send()
            .await?;
    }

    Ok(())
}
