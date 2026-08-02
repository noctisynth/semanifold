#![cfg_attr(test, allow(clippy::unwrap_used))]

use clap::Parser;
use log::LevelFilter;
use rust_i18n::t;
use semifold_engine::ProjectLocation;

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

    log::debug!("Parsed CLI arguments: {:?}", cli);

    if let Some(Commands::Mcp(mcp)) = &cli.command {
        utils::run_async(cli::mcp::run_mcp(mcp))?;
        return Ok(());
    }

    let current_dir = std::env::current_dir()?;
    let changeset_dir = std::env::var_os("CHANGESET_PATH").map(std::path::PathBuf::from);
    let location =
        ProjectLocation::discover_with_changeset_dir(&current_dir, changeset_dir.as_deref())
            .map_err(|error| anyhow::anyhow!(t!("cli.project_load_failed", error = error)))?;
    if let Some(Commands::Init(init)) = &cli.command {
        cli::init::run(init, &location)?;
        return Ok(());
    }
    let project = location
        .load()
        .map_err(|error| anyhow::anyhow!(t!("cli.project_load_failed", error = error)))?;
    log::debug!("Loaded config: {:?}", project.config);

    match &cli.command {
        Some(Commands::Commit(commit)) => cli::commit::run(commit, &project)?,
        Some(Commands::Config(config)) => cli::config::run(config, &project, cli.dry_run)?,
        Some(Commands::Version(version)) => {
            utils::run_async(cli::version::run(version, &project, cli.dry_run))?
        }
        Some(Commands::Publish(publish)) => {
            utils::run_async(cli::publish::run(publish, &project, cli.dry_run))?
        }
        Some(Commands::CI(ci)) => utils::run_async(cli::ci::run(ci, &project, cli.dry_run))?,
        Some(Commands::Status(status)) => utils::run_async(cli::status::run(status, &project))?,
        Some(Commands::Init(_) | Commands::Mcp(_)) | None => {}
    }

    Ok(())
}
