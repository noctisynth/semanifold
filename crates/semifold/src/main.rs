use clap::Parser;
use colored::Colorize;
use log::LevelFilter;
use semifold_resolver::context;

use crate::cli::{Cli, Commands};

pub mod cli;
pub mod logger;
pub mod utils;

rust_i18n::i18n!("locales", fallback = "en");

fn run() -> anyhow::Result<()> {
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
        Some(Commands::Version(version)) => cli::version::run(version, &ctx)?,
        Some(Commands::Publish(publish)) => cli::publish::run(publish, &ctx)?,
        Some(Commands::CI(ci)) => utils::run_async(cli::ci::run(ci, &ctx))?,
        Some(Commands::Status(status)) => utils::run_async(cli::status::run(status, &ctx))?,
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
