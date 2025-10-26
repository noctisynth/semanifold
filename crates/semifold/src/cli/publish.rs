use clap::Parser;
use rust_i18n::t;
use semifold_resolver::context::Context;

#[derive(Debug, Parser)]
pub(crate) struct Publish {
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

pub(crate) fn run(opts: &Publish, ctx: &Context) -> anyhow::Result<()> {
    if !ctx.is_initialized() {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    publish(ctx, opts.dry_run)?;

    Ok(())
}
