use clap::Parser;
use rust_i18n::t;
use semanifold_resolver::{config::Config, context::Context};

#[derive(Debug, Parser)]
pub(crate) struct Publish {
    /// Whether to publish the package
    #[clap(long)]
    dry_run: bool,
}

pub(crate) fn publish(config: &Config, dry_run: bool) -> anyhow::Result<()> {
    let packages = config.packages.values().collect::<Vec<_>>();

    log::debug!("Packages to publish: {:?}", &packages);

    for package in packages {
        let mut resolver = package.resolver.get_resolver();
        let resolved_package = resolver.resolve(package)?;
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

pub(crate) fn run(opts: &Publish, ctx: &Context) -> anyhow::Result<()> {
    let Context {
        config: Some(config),
        ..
    } = ctx
    else {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    publish(config, opts.dry_run)?;

    Ok(())
}
