use clap::Parser;
use log::LevelFilter;
use semifold_resolver::context;

pub mod cli;
pub mod logger;
pub mod utils;

use cli::{Cli, Commands};

rust_i18n::i18n!("locales", fallback = "en");

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        logger::setup_logger(LevelFilter::Debug)?;
    } else {
        logger::setup_logger(LevelFilter::Info)?;
    }

    log::debug!("Parsed CLI arguments: {:?}", &cli);

    let ctx = context::Context::create()?;

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
