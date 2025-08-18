use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct PackageConfig {
    pub path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub tags: HashMap<String, String>,
    pub packages: HashMap<String, PackageConfig>,
}

pub fn get_config_path(changeset_path: &Path) -> anyhow::Result<PathBuf> {
    let config_paths = ["config.toml", "config.json"];
    let config_path = config_paths
        .iter()
        .find_map(|path| {
            let config_path = changeset_path.join(path);
            if config_path.exists() {
                Some(config_path)
            } else {
                None
            }
        })
        .ok_or(anyhow::anyhow!("Failed to find config file"))?;

    log::debug!("Found config path: {config_path:?}");

    Ok(config_path)
}

pub fn load_config(config_path: &Path) -> anyhow::Result<Config> {
    let config_content = std::fs::read_to_string(config_path)?;
    let config = if config_path.extension() == Some(OsStr::new("toml")) {
        toml::from_str(&config_content)?
    } else {
        serde_json::from_str(&config_content)?
    };
    Ok(config)
}
