use std::path::PathBuf;

use clap::{Args, arg};
use log::info;

#[derive(Debug, Args)]
pub(crate) struct Init {
    #[arg(short, long, default_value = "./.changes")]
    pub root: PathBuf,
    #[arg(short, long, default_value = "true")]
    pub tag: bool,
}

pub(crate) fn run(init: &Init) -> anyhow::Result<()> {
    std::fs::create_dir_all(&init.root)?;

    // init config.toml file
    let config_path = init.root.join("config.toml");

    let mut table = toml::Table::new();
    // init packages
    let mut packages = toml::Table::new();
    packages.insert(
        "semanifold".to_string(),
        toml::Value::Table({
            let mut tbl = toml::value::Table::new();
            tbl.insert(
                "path".to_string(),
                toml::Value::String("crates/semanifold".to_string()),
            );
            tbl
        }),
    );
    packages.insert(
        "semanifold-resolver".to_string(),
        toml::Value::Table({
            let mut tbl = toml::value::Table::new();
            tbl.insert(
                "path".to_string(),
                toml::Value::String("crates/resolver".to_string()),
            );
            tbl
        }),
    );
    table.insert("packages".to_string(), toml::Value::Table(packages));
    // init tags
    let mut tags = toml::Table::new();
    if init.tag {
        tags.insert(
            "feat".to_string(),
            toml::Value::String("New Feature".to_string()),
        );
        tags.insert(
            "fix".to_string(),
            toml::Value::String("Bug Fix".to_string()),
        );
        tags.insert(
            "chore".to_string(),
            toml::Value::String("Chore".to_string()),
        );
        tags.insert(
            "refactor".to_string(),
            toml::Value::String("Refactor".to_string()),
        );
        tags.insert(
            "perf".to_string(),
            toml::Value::String("Performance Improvement".to_string()),
        );
    }
    table.insert("tags".to_string(), toml::Value::Table(tags));

    std::fs::write(config_path, toml::to_string_pretty(&table)?)?;
    info!("Initialized semanifold in {}", init.root.display());
    Ok(())
}
