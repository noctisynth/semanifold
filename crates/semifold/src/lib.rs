#![cfg_attr(test, allow(clippy::unwrap_used))]

use clap::Parser;
use log::LevelFilter;
use semifold_resolver::context;

pub mod cli;
pub(crate) mod config_editor;
pub(crate) mod config_sync;
pub(crate) mod discovery;
pub mod file_edit_executor;
pub mod logger;
pub(crate) mod package_path;
pub(crate) mod release;
pub mod utils;
pub mod workspace;

use cli::{Cli, Commands};

rust_i18n::i18n!("locales", fallback = "en");

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        logger::setup_logger(LevelFilter::Debug)?;
    } else {
        logger::setup_logger(LevelFilter::Info)?;
    }

    log::debug!("Parsed CLI arguments: {:?}", cli);

    let mut ctx = context::Context::create()?;
    ctx.dry_run(cli.dry_run);

    log::debug!("Loaded config: {:?}", ctx.config);

    match &cli.command {
        Some(Commands::Commit(commit)) => cli::commit::run(commit, &ctx)?,
        Some(Commands::Init(init)) => cli::init::run(init, &ctx)?,
        Some(Commands::Config(config)) => cli::config::run(config, &ctx)?,
        Some(Commands::Version(version)) => utils::run_async(cli::version::run(version, &ctx))?,
        Some(Commands::Publish(publish)) => utils::run_async(cli::publish::run(publish, &ctx))?,
        Some(Commands::CI(ci)) => utils::run_async(cli::ci::run(ci, &ctx))?,
        Some(Commands::Status(status)) => utils::run_async(cli::status::run(status, &ctx))?,
        Some(Commands::Mcp(mcp)) => utils::run_async(cli::mcp::run_mcp(mcp))?,
        None => {}
    }

    Ok(())
}
