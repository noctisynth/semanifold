use std::{
    collections::HashMap,
    process::{Command, Stdio},
};

use clap::Parser;
use colored::Colorize;
use git2::Repository;
use rust_i18n::t;
use semifold_changelog::{generate_changelog, utils::insert_changelog};
use semifold_resolver::{
    changeset::{BumpLevel, Changeset},
    config::ResolverConfig,
    context::Context,
    resolver, utils,
};

#[derive(Parser, Debug)]
pub(crate) struct Version {
    #[clap(long, help = "Print the version bumps without applying them")]
    dry_run: bool,
}

pub(crate) fn post_version(ctx: &Context) -> anyhow::Result<()> {
    let packages = ctx.get_packages();
    for (package_name, package_config) in packages {
        let resolver_config = ctx.get_resolver_config(package_config.resolver);
        if let Some(ResolverConfig { post_version, .. }) = &resolver_config {
            for command in post_version {
                Command::new(&command.command)
                    .args(command.args.clone().unwrap_or_default())
                    .stdout(Stdio::inherit())
                    .status()?;
            }
        } else {
            log::warn!(
                "Failed to get post version commands for package: {}",
                package_name
            );
        }
    }
    Ok(())
}

pub(crate) async fn version(
    ctx: &Context,
    changesets: &[Changeset],
    dry_run: bool,
) -> anyhow::Result<HashMap<String, String>> {
    let config = ctx.config.as_ref().unwrap();
    let root = ctx.repo_root.as_ref().unwrap();
    let repo = Repository::open(root)?;
    let mut changelogs_map = HashMap::new();

    for (package_name, package_config) in &config.packages {
        log::debug!("Processing package: {}", package_name);
        let mut resolver = ctx.create_resolver(package_config.resolver);
        let resolved_package = resolver.resolve(root, package_config)?;
        let level = utils::get_bump_level(changesets, package_name);

        // Skip unchanged packages
        if matches!(level, BumpLevel::Unchanged) {
            log::warn!("{} is unchanged, skip versioning", package_name.cyan());
            continue;
        }

        let bumped_version = utils::bump_version(&resolved_package.version, level)?;
        resolver.bump(root, &resolved_package, &bumped_version, dry_run)?;

        let changelog = generate_changelog(
            ctx,
            &repo,
            changesets,
            package_name,
            &bumped_version.to_string(),
        )
        .await?;
        changelogs_map.insert(package_name.to_string(), changelog.clone());

        log::debug!("changelog for {}:\n{}", package_name, changelog);

        if !dry_run {
            insert_changelog(
                root.join(&package_config.path).join("CHANGELOG.md"),
                &changelog,
            )
            .await?;
        }
    }

    if !dry_run {
        changesets.iter().try_for_each(|c| c.clean())?;
        post_version(ctx)?;
    }
    Ok(changelogs_map)
}

pub(crate) async fn run(opts: &Version, ctx: &Context) -> anyhow::Result<()> {
    if !ctx.is_initialized() {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    let changesets = resolver::get_changesets(ctx)?;
    if changesets.is_empty() {
        log::warn!("No changesets found, skip versioning");
        return Ok(());
    }

    version(ctx, &changesets, opts.dry_run).await?;

    Ok(())
}
