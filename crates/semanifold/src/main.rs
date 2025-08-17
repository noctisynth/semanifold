use clap::Parser;
use log::LevelFilter;
use semanifold_resolver::config;

use crate::cli::{Cli, Commands};

pub mod cli;
pub mod logger;

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        logger::setup_logger(LevelFilter::Debug)?;
    } else {
        logger::setup_logger(LevelFilter::Info)?;
    }

    log::debug!("Parsed CLI arguments: {:?}", &cli);

    let config_path = config::get_config_path()?;
    let config = config::load_config(&config_path)?;

    log::debug!("Loaded config: {:?}", &config);

    match &cli.command {
        Some(Commands::Add(add)) => cli::add::run(add, &config)?,
        None => {}
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        log::error!("Error: {}", e);
    }
}
