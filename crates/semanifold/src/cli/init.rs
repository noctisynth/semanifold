use std::{fmt::Debug, path::PathBuf};

use clap::{Args, arg};
use log::info;
use toml_edit::{DocumentMut, Table, value};

fn find_rust_package(root: &PathBuf) -> anyhow::Result<Table> {
    let mut packages = Table::new();
    let config_path = root.join("Cargo.toml");

    let config = std::fs::read_to_string(config_path)
        .map_err(|_| anyhow::Error::msg("Cargo.toml not found in root"))?;

    let doc = config.parse::<DocumentMut>()?;

    if let Some(pkg) = doc.get("package").and_then(|v| v.as_table()) {
        packages[pkg["name"].as_str().unwrap()]["path"] = value(root.to_str().unwrap());
    } else if let Some(workspace) = doc.get("workspace").and_then(|v| v.as_table()) {
        if let Some(members) = workspace.get("members").and_then(|v| v.as_array()) {
            for member in members {
                if let Some(pattern) = member.as_str() {
                    let paths = glob::glob(&root.join(pattern).to_string_lossy())?;
                    for entry in paths {
                        let member_path = entry?;
                        if member_path.join("Cargo.toml").exists() {
                            log::info!("Found workspace member: {}", member_path.display());
                            let pkg = find_rust_package(&member_path)?;
                            packages.extend(pkg);
                        }
                    }
                }
            }
        }
    }

    Ok(packages)
}

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

    let packages = find_rust_package(&init.root.parent().unwrap().to_path_buf())?;
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
