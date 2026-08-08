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

    log::debug!(
        "Parsed command: {:?}",
        cli.command.as_ref().map(command_name)
    );

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
    log::debug!(
        "Loaded project with {} configured package(s)",
        project.config.packages.len()
    );

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

fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Commit(_) => "commit",
        Commands::Init(_) => "init",
        Commands::Config(_) => "config",
        Commands::Version(_) => "version",
        Commands::Publish(_) => "publish",
        Commands::CI(_) => "ci",
        Commands::Status(_) => "status",
        Commands::Mcp(_) => "mcp",
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, str::FromStr};

    use toml_edit::{DocumentMut, Item, Table};

    fn locale_keys(source: &str) -> BTreeSet<String> {
        let document = DocumentMut::from_str(source).expect("locale fixture is valid TOML");
        let mut keys = BTreeSet::new();
        collect_keys(document.as_table(), "", &mut keys);
        keys
    }

    fn collect_keys(table: &Table, prefix: &str, keys: &mut BTreeSet<String>) {
        for (name, item) in table {
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}.{name}")
            };
            match item {
                Item::Table(child) => collect_keys(child, &path, keys),
                Item::Value(_) | Item::ArrayOfTables(_) | Item::None => {
                    keys.insert(path);
                }
            }
        }
    }

    #[test]
    fn english_and_chinese_locales_have_identical_keys() {
        assert_eq!(
            locale_keys(include_str!("../locales/en.toml")),
            locale_keys(include_str!("../locales/zh.toml"))
        );
    }

    #[test]
    fn debug_paths_do_not_dump_configuration_or_github_payloads() {
        let config_dump = ["Loaded ", "config:"].concat();
        let event_dump = ["GITHUB_EVENT_PATH", " data:"].concat();
        assert!(!include_str!("lib.rs").contains(&config_dump));
        assert!(!include_str!("cli/status.rs").contains(&event_dump));
    }
}
