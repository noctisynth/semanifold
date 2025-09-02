use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{error::ResolveError, resolver};

#[derive(Serialize, Deserialize, Debug)]
pub struct PackageConfig {
    pub path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub tags: HashMap<String, String>,
    pub packages: HashMap<String, PackageConfig>,
}

pub fn get_config_path(changeset_path: &Path) -> Result<PathBuf, ResolveError> {
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
        .ok_or(ResolveError::FileOrDirNotFound {
            path: "config.toml".into(),
        })?;

    log::debug!("Found config path: {config_path:?}");

    Ok(config_path)
}

pub fn load_config(config_path: &Path) -> Result<Config, ResolveError> {
    let config_content = std::fs::read_to_string(config_path)?;
    let config = if config_path.extension() == Some(OsStr::new("toml")) {
        toml::from_str(&config_content).map_err(|e| ResolveError::InvalidConfig {
            path: config_path.to_path_buf(),
            reason: e.to_string(),
        })?
    } else {
        serde_json::from_str(&config_content).map_err(|e| ResolveError::InvalidConfig {
            path: config_path.to_path_buf(),
            reason: e.to_string(),
        })?
    };
    Ok(config)
}

pub fn get_config() -> Result<Config, ResolveError> {
    let changeset_path = resolver::get_changeset_path()?;
    let config_path = get_config_path(&changeset_path)?;
    load_config(&config_path)
}
