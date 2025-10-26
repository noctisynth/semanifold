use clap::Parser;
use colored::Colorize;
use rust_i18n::t;

use semifold_changelog::read_latest_changelog;
use semifold_resolver::{config::PackageConfig, context::Context};

#[derive(Debug, Parser)]
pub(crate) struct Publish {
    /// Whether to create a GitHub release, only available when running in CI
    #[clap(short = 'r', long, default_value_t = true)]
    github_release: bool,
    /// Whether to publish the package
    #[clap(long)]
    dry_run: bool,
}

pub(crate) async fn create_github_release(
    ctx: &Context,
    octocrab: &octocrab::Octocrab,
    package_name: &str,
    package_config: &PackageConfig,
    releases: &[octocrab::models::repos::Release],
) -> anyhow::Result<()> {
    let Some(repo_info) = &ctx.repo_info else {
        return Err(anyhow::anyhow!("Repo info not found"));
    };

    let changelog_path = package_config.path.join("CHANGELOG.md");
    if !changelog_path.exists() {
        log::warn!(
            "Changelog file not found for package {}, skip create GitHub release",
            &package_name.cyan()
        );
        return Ok(());
    }

    let changelog = read_latest_changelog(&changelog_path).await?;
    let tag_name = format!("{}-{}", package_name, changelog.version);
    let release_title = format!("{} {}", package_name, changelog.version);

    log::debug!("Tag name: {}", &tag_name);
    log::debug!("Changelog for {}:\n\n{}", &package_name, &changelog.body);

    if releases.iter().any(|release| release.tag_name == tag_name) {
        log::warn!("Release {} already exists", &tag_name);
        return Ok(());
    }

    octocrab
        .repos(&repo_info.owner, &repo_info.repo_name)
        .releases()
        .create(&tag_name)
        .name(&release_title)
        .body(&changelog.body)
        .send()
        .await?;

    Ok(())
}

pub(crate) async fn publish(
    ctx: &Context,
    github_release: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let config = ctx.config.as_ref().unwrap();

    log::debug!(
        "Packages to publish: {:?}",
        &config.packages.keys().collect::<Vec<_>>()
    );

    let should_create_github_release = ctx.is_ci() && github_release;

    let octocrab = if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        octocrab::Octocrab::builder()
            .personal_token(token)
            .build()?
    } else {
        octocrab::Octocrab::default()
    };
    let releases = if should_create_github_release {
        let Some(repo_info) = &ctx.repo_info else {
            return Err(anyhow::anyhow!("Git repository is not initialized"));
        };

        octocrab
            .repos(&repo_info.owner, &repo_info.repo_name)
            .releases()
            .list()
            .send()
            .await?
            .take_items()
    } else {
        Vec::new()
    };

    let mut sorted_packages = config.packages.clone().into_iter().collect::<Vec<_>>();
    for resolver in config.resolver.keys() {
        resolver
            .get_resolver()
            .sort_packages(&mut sorted_packages)?;
    }
    log::debug!("Sorted packages: {:?}", &sorted_packages);

    let root = ctx.repo_root.clone().unwrap_or(std::env::current_dir()?);
    for (package_name, package) in &sorted_packages {
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

        if should_create_github_release {
            if !dry_run {
                create_github_release(ctx, &octocrab, package_name, package, &releases).await?;
            } else {
                log::warn!(
                    "Dry run, not creating GitHub release for {} {}",
                    &package_name.cyan(),
                    &format!("v{}", resolved_package.version).green()
                );
            }
        }
    }

    Ok(())
}

pub(crate) async fn run(opts: &Publish, ctx: &Context) -> anyhow::Result<()> {
    if !ctx.is_initialized() {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    publish(ctx, opts.github_release, opts.dry_run).await?;

    Ok(())
}
