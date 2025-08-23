use std::{fmt::Debug, path::PathBuf};

use clap::{Args, arg};
use log::info;
use toml_edit::{DocumentMut, Table, value};

fn find_rust_package(root: &PathBuf) -> anyhow::Result<Table> {
    let mut packages = Table::new();

    // Case-insensitive file lookup
    let config_path = root
        .read_dir()?
        .filter_map(Result::ok)
        .find(|entry| {
            entry.path().is_file()
                && entry
                    .file_name()
                    .to_str()
                    .map(|name| name.eq_ignore_ascii_case("Cargo.toml"))
                    .unwrap_or(false)
        })
        .map(|entry| entry.path());

    let Some(config_path) = config_path else {
        log::warn!("在{}目录未找到Cargo.toml文件", root.display());
        return Ok(packages);
    };

    let doc = std::fs::read_to_string(config_path)?.parse::<DocumentMut>()?;

    if let Some(pkg) = doc.get("package").and_then(|v| v.as_table()) {
        match pkg["name"].as_str() {
            Some(name) => packages[name]["path"] = value(root.to_str().unwrap()),
            None => {
                log::warn!("{}cargo缺少name字段", root.display());
                return Ok(packages);
            }
        }
        return Ok(packages);
    }

    let Some(workspace) = doc.get("workspace").and_then(|v| v.as_table()) else {
        return Ok(packages);
    };
    let Some(members) = workspace.get("members").and_then(|v| v.as_array()) else {
        return Ok(packages);
    };

    members
        .iter()
        .filter_map(|m| m.as_str())
        .flat_map(|pattern| glob::glob(&root.join(pattern).to_string_lossy()).ok())
        .flatten()
        .filter_map(Result::ok)
        .for_each(|entry| {
            log::info!("Found workspace member: {}", entry.display());
            if let Ok(pkg) = find_rust_package(&entry) {
                packages.extend(pkg);
            }
        });

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
