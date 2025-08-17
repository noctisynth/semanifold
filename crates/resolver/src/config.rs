use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PackageConfig {
    path: String,
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub tags: HashMap<String, String>,
    pub packages: HashMap<String, PackageConfig>,
}

pub fn find_at_parent(path_name: &str, starts_at: &Path, ends_at: Option<&Path>) -> Option<PathBuf> {
    let mut current_path = starts_at;
    loop {
        if ends_at.is_some() && current_path == ends_at.unwrap() {
            break None;
        } else {
            let config_path = current_path.join(path_name);
            if config_path.exists() {
                break Some(config_path);
            }
        }
        if let Some(parent_path) = current_path.parent() {
            current_path = parent_path;
        } else {
            break None;
        }
    }
}

pub fn get_config_path() -> anyhow::Result<PathBuf> {
    let current_path = if let Ok(config_path) = std::env::current_dir() {
        config_path
    } else {
        return Err(anyhow::anyhow!("Failed to get current directory"));
    };

    let changeset_path = if let Ok(changeset_path) = std::env::var("CHANGESET_PATH") {
        PathBuf::from(changeset_path)
    } else {
        let changeset_dirs = [".changesets", ".changes"];
        changeset_dirs
            .iter()
            .find_map(|dir| find_at_parent(dir, &current_path, None))
            .ok_or(anyhow::anyhow!("Failed to find changeset directory"))?
    };

    let config_paths = ["config.toml", "config.json"];
    let config_path = config_paths
        .iter()
        .find_map(|path| {
            let config_path = changeset_path.join(path);
            log::debug!("Checking config path: {:?}", config_path);
            if config_path.exists() {
                Some(config_path)
            } else {
                None
            }
        })
        .ok_or(anyhow::anyhow!("Failed to find config file"))?;

    Ok(config_path)
}

pub fn load_config() -> anyhow::Result<Config> {
    let config_path = get_config_path()?;
    let config_content = std::fs::read_to_string(&config_path)?;
    let config = if config_path.extension() == Some(OsStr::new("toml")) {
        toml::from_str(&config_content)?
    } else {
        serde_json::from_str(&config_content)?
    };
    Ok(config)
}
