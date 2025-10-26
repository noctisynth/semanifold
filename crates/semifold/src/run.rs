use clap::Parser;
use log::LevelFilter;
use semifold_resolver::context;

use crate::cli::{Cli, Commands};
use crate::{cli, logger, utils};

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        logger::setup_logger(LevelFilter::Debug)?;
    } else {
        logger::setup_logger(LevelFilter::Info)?;
    }

    log::debug!("Parsed CLI arguments: {:?}", &cli);

    let ctx = context::Context::create().unwrap_or_default();

    log::debug!("Loaded config: {:?}", &ctx.config);

    match &cli.command {
        Some(Commands::Commit(commit)) => cli::commit::run(commit, &ctx)?,
        Some(Commands::Init(init)) => cli::init::run(init, &ctx)?,
        Some(Commands::Version(version)) => utils::run_async(cli::version::run(version, &ctx))?,
        Some(Commands::Publish(publish)) => utils::run_async(cli::publish::run(publish, &ctx))?,
        Some(Commands::CI(ci)) => utils::run_async(cli::ci::run(ci, &ctx))?,
        Some(Commands::Status(status)) => utils::run_async(cli::status::run(status, &ctx))?,
        None => {}
    }

    Ok(())
}
