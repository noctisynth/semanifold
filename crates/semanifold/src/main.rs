use clap::Parser;
use log::LevelFilter;
use semanifold_resolver::{config, resolver};

use crate::cli::{Cli, Commands};

pub mod cli;
pub mod i18n;
pub mod logger;

rust_i18n::i18n!();

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        logger::setup_logger(LevelFilter::Debug)?;
    } else {
        logger::setup_logger(LevelFilter::Info)?;
    }

    log::debug!("Parsed CLI arguments: {:?}", &cli);

    let changeset_path = resolver::get_changeset_path()?;
    let config_path = config::get_config_path(&changeset_path)?;
    let config = config::load_config(&config_path)?;

    log::debug!("Loaded config: {:?}", &config);

    match &cli.command {
        Some(Commands::Add(add)) => cli::add::run(add, &changeset_path, &config)?,
        Some(Commands::Init(init)) => cli::init::run(init)?,
        None => {}
    }

    Ok(())
}

fn main() {
    i18n::init();
    if let Err(e) = run() {
        log::error!("Error: {e}");
    }
}
