use clap::Parser;
use colored::Colorize;
use log::LevelFilter;
use semanifold_resolver::{config, resolver};

use crate::cli::{Cli, Commands};

pub mod cli;
pub mod logger;

rust_i18n::i18n!("locales", fallback = "en");

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        logger::setup_logger(LevelFilter::Debug)?;
    } else {
        logger::setup_logger(LevelFilter::Info)?;
    }

    // TODO: refactor to context based
    log::debug!("Parsed CLI arguments: {:?}", &cli);
    // init command must be executed before read config file
    if let Some(Commands::Init(init)) = &cli.command {
        return cli::init::run(init);
    }

    let changeset_path = resolver::get_changeset_path()?;
    let config_path = config::get_config_path(&changeset_path)?;
    let config = config::load_config(&config_path)?;

    log::debug!("Loaded config: {:?}", &config);

    match &cli.command {
        Some(Commands::Commit(commit)) => cli::commit::run(commit, &changeset_path, &config)?,
        Some(Commands::Init(_init)) => {}
        None => {}
    }

    Ok(())
}

fn main() {
    if let Some(locale) = sys_locale::get_locale() {
        rust_i18n::set_locale(&locale);
    }

    if let Err(e) = run() {
        log::error!("{}", e.to_string().red());
        std::process::exit(1);
    }
}
