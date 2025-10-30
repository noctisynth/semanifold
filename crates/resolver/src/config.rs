use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{error::ResolveError, resolver};

#[derive(Serialize, Deserialize, Debug)]
pub struct BranchesConfig {
    pub base: String,
    pub release: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackageConfig {
    pub path: PathBuf,
    pub resolver: resolver::ResolverType,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum StdioType {
    #[default]
    Inherit,
    Pipe,
    Null,
}

impl StdioType {
    pub fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }
}

impl From<StdioType> for std::process::Stdio {
    fn from(value: StdioType) -> Self {
        match value {
            StdioType::Inherit => Self::inherit(),
            StdioType::Pipe => Self::piped(),
            StdioType::Null => Self::null(),
        }
    }
}

/// Configuration for a command to run.
#[derive(Serialize, Deserialize, Debug)]
pub struct CommandConfig {
    /// Executable command to run.
    pub command: String,
    /// Arguments to pass to the command.
    pub args: Option<Vec<String>>,
    /// Environment variables to set before running the command.
    #[serde(
        default,
        rename = "extra-env",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub extra_env: BTreeMap<String, String>,
    /// Type of standard output to use.
    #[serde(default, skip_serializing_if = "StdioType::is_inherit")]
    pub stdout: StdioType,
    /// Type of standard error to use.
    #[serde(default, skip_serializing_if = "StdioType::is_inherit")]
    pub stderr: StdioType,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PreCheckConfig {
    pub url: String,
    #[serde(
        default,
        rename = "extra-headers",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub extra_headers: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResolverConfig {
    /// Pre-check configuration.
    #[serde(rename = "pre-check")]
    pub pre_check: PreCheckConfig,
    /// Commands to run before publish.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prepublish: Vec<CommandConfig>,
    /// Commands to run for publish.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publish: Vec<CommandConfig>,
    /// Commands to run after versioning.
    #[serde(
        default,
        rename = "post-version",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub post_version: Vec<CommandConfig>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub branches: BranchesConfig,
    pub tags: BTreeMap<String, String>,
    pub packages: BTreeMap<String, PackageConfig>,
    pub resolver: BTreeMap<resolver::ResolverType, ResolverConfig>,
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
        toml_edit::de::from_str(&config_content).map_err(|e| ResolveError::InvalidConfig {
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

pub fn save_config(config_path: &Path, config: &Config) -> Result<(), ResolveError> {
    let config_content = if config_path.extension() == Some(OsStr::new("toml")) {
        toml_edit::ser::to_string_pretty(config).map_err(|e| ResolveError::InvalidConfig {
            path: config_path.to_path_buf(),
            reason: e.to_string(),
        })?
    } else {
        serde_json::to_string(config).map_err(|e| ResolveError::InvalidConfig {
            path: config_path.to_path_buf(),
            reason: e.to_string(),
        })?
    };
    std::fs::write(config_path, config_content)?;
    Ok(())
}
