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
    #[clap(long, help = t!("cli.version.flags.allow_dirty"))]
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
                        "{}",
                        t!(
                            "cli.version.skip_post_version",
                            command = format!("{} {}", command.command, args.join(" ")).magenta(),
                            package = package_name.cyan()
                        )
                    );
                    continue;
                }

                log::info!(
                    "{}",
                    t!(
                        "cli.version.run_post_version",
                        command = format!("{} {}", command.command, args.join(" ")).magenta(),
                        package = package_name.cyan()
                    )
                );
                utils::run_command(command, &package_config.path)?;
            }
        } else {
            log::warn!(
                "{}",
                t!(
                    "cli.version.no_resolver_config",
                    resolver = package_config.resolver.to_string().cyan(),
                    package = package_name.cyan()
                )
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
        return Err(anyhow::anyhow!(t!("cli.version.no_git_repo")));
    };
    let mut changelogs_map = HashMap::new();

    let mut sorted_packages = config.packages.clone().into_iter().collect::<Vec<_>>();
    for resolver in config.resolver.keys() {
        ctx.create_resolver(*resolver)
            .sort_packages(root, &mut sorted_packages)?;
    }
    for (package_name, package_config) in &sorted_packages {
        log::debug!("Processing package: {}", package_name);
        let mut resolver = ctx.create_resolver(package_config.resolver);
        let resolved_package = resolver.resolve(root, package_config)?;
        let level = utils::get_bump_level(changesets, package_name);

        // Skip unchanged packages
        if matches!(level, BumpLevel::Unchanged) {
            log::warn!(
                "{}",
                t!("cli.version.unchanged", package = package_name.cyan())
            );
            continue;
        }

        let mut bumped_version = resolved_package.version.clone();
        utils::bump_version(&mut bumped_version, level, &package_config.version_mode)?;
        resolver.bump(ctx, root, &resolved_package, &bumped_version)?;
        ctx.version_bumps
            .borrow_mut()
            .entry(package_name.clone())
            .or_insert(bumped_version.clone());

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
        log::warn!("{}", t!("cli.version.empty_changesets"));
        return Ok(());
    }

    version(ctx, &changesets).await?;

    Ok(())
}
