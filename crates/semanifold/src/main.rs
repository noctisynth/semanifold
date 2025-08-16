use clap::Parser;
use logger::LevelFilter;

use crate::cli::{Cli, Commands};

pub mod cli;
pub mod logger;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        logger::setup_logger(LevelFilter::Debug)?;
    } else {
        logger::setup_logger(LevelFilter::Info)?;
    }

    match &cli.command {
        Some(Commands::Add(add)) => cli::add::run(add)?,
        None => {}
    }

    Ok(())
}
