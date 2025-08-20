use std::{fmt::Debug, path::PathBuf};

use clap::{Args, arg};
use log::info;
use toml_edit::{DocumentMut, Table, value};

#[derive(Debug, Args)]
pub(crate) struct Init {
    #[arg(short, long, default_value = "./.changes")]
    pub root: PathBuf,
}

pub(crate) fn run(init: &Init) -> anyhow::Result<()> {
    std::fs::create_dir_all(&init.root)?;

    // init config.toml file
    let config_path = init.root.join("config.toml");

    let mut doc = DocumentMut::new();
    //init packages
    let mut packages = Table::new();
    packages["semanifold"]["path"] = value("crates/semanifold");
    packages["semanifold-resolver"]["path"] = value("crates/resolver");
    doc["packages"] = toml_edit::Item::Table(packages);
    // init tags
    let mut tags = Table::new();
    tags["chore"] = value("Chore");
    tags["feat"] = value("New Feature");
    tags["fix"] = value("Bug Fix");
    tags["perf"] = value("Performance Improvement");
    tags["refactor"] = value("Refactor");
    doc["tags"] = toml_edit::Item::Table(tags);
    // write config.toml file
    std::fs::write(config_path, doc.to_string())?;
    info!("Initialized semanifold in {}", init.root.display());
    Ok(())
}
