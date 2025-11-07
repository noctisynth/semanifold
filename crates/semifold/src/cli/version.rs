use std::collections::HashMap;

use clap::Parser;
use colored::Colorize;
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
    #[clap(long, help = "Allow versioning packages with dirty git working tree")]
    allow_dirty: bool,
}

pub(crate) fn post_version(ctx: &Context) -> anyhow::Result<()> {
    let packages = ctx.get_packages();
    for (package_name, package_config) in packages {
        let resolver_config = ctx.get_resolver_config(package_config.resolver);
        if let Some(ResolverConfig { post_version, .. }) = &resolver_config {
            for command in post_version {
                let args = command.args.as_deref().unwrap_or_default();
                if ctx.dry_run && !command.dry_run.unwrap_or(false) {
                    log::warn!(
                        "Skipping post version command {} {} for package {} due to dry run",
                        command.command.magenta(),
                        args.join(" ").magenta(),
                        package_name.cyan()
                    );
                    continue;
                }

                log::info!(
                    "Running post version command {} {} for package {}",
                    command.command.magenta(),
                    args.join(" ").magenta(),
                    package_name.cyan()
                );
                utils::run_command(command, &package_config.path)?;
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
) -> anyhow::Result<HashMap<String, String>> {
    let config = ctx.config.as_ref().unwrap();
    let root = ctx.repo_root.as_ref().unwrap();
    let Some(repo) = ctx.git_repo.as_ref() else {
        return Err(anyhow::anyhow!("Failed to open Git repository"));
    };
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

        let mut bumped_version = resolved_package.version.clone();
        utils::bump_version(&mut bumped_version, level, &package_config.version_mode)?;
        resolver.bump(root, &resolved_package, &bumped_version, ctx.dry_run)?;

        let changelog = generate_changelog(
            ctx,
            repo,
            changesets,
            package_name,
            &bumped_version.to_string(),
        )
        .await?;
        changelogs_map.insert(package_name.to_string(), changelog.clone());

        log::debug!("changelog for {}:\n{}", package_name, changelog);

        if !ctx.dry_run {
            insert_changelog(
                root.join(&package_config.path).join("CHANGELOG.md"),
                &changelog,
            )
            .await?;
        }
    }

    if !ctx.dry_run {
        changesets.iter().try_for_each(|c| c.clean())?;
    }
    post_version(ctx)?;

    Ok(changelogs_map)
}

pub(crate) async fn run(opts: &Version, ctx: &Context) -> anyhow::Result<()> {
    if !ctx.is_initialized() {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    if !opts.allow_dirty && !ctx.is_git_repo_clean() {
        return Err(anyhow::anyhow!(t!("cli.dirty_repo")));
    }

    let changesets = resolver::get_changesets(ctx)?;
    if changesets.is_empty() {
        log::warn!("No changesets found, skip versioning");
        return Ok(());
    }

    version(ctx, &changesets).await?;

    Ok(())
}
