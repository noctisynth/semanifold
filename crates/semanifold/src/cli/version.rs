use std::{cmp::max, path::Path};

use clap::Parser;
use rust_i18n::t;
use semanifold_resolver::{
    changeset::BumpLevel, config::Config, context::Context, resolver, utils,
};

#[derive(Parser, Debug)]
pub(crate) struct Version {
    #[clap(long, help = "Print the version bumps without applying them")]
    dry_run: bool,
}

pub(crate) fn version(config: &Config, changeset_root: &Path, dry_run: bool) -> anyhow::Result<()> {
    for (package_name, package_config) in &config.packages {
        let mut resolver = package_config.resolver.get_resolver();
        let resolved_package = resolver.resolve(package_config)?;

        let changesets = resolver::get_changesets(changeset_root)?;
        let mut level = BumpLevel::Patch;
        for changeset in changesets {
            changeset.packages.iter().for_each(|package| {
                if &package.name == package_name {
                    level = max(level, package.level);
                    log::debug!("Bump level of {} to {:?}", package_name, level);
                }
            });
        }

        let bumped_version = utils::bump_version(&resolved_package.version, level)?;
        resolver.bump(&resolved_package, &bumped_version, dry_run)?;
    }
    Ok(())
}

pub(crate) fn run(opts: &Version, ctx: &Context) -> anyhow::Result<()> {
    let Context {
        config: Some(config),
        changeset_root: Some(changeset_root),
        ..
    } = ctx
    else {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    version(config, changeset_root, opts.dry_run)?;

    Ok(())
}
