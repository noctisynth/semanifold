use std::{env, path::PathBuf};

use crate::{config, error, resolver};

#[derive(Default)]
pub struct Context {
    pub config: Option<config::Config>,
    pub changeset_root: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub repo_root: Option<PathBuf>,
}

impl Context {
    pub fn create() -> Result<Self, error::ResolveError> {
        let changeset_root = resolver::get_changeset_path()?;
        let config_path = config::get_config_path(&changeset_root)?;
        let config = config::load_config(&config_path)?;
        let repo_root = resolver::get_repo_root().ok();

        Ok(Self {
            config: Some(config),
            changeset_root: Some(changeset_root),
            config_path: Some(config_path),
            repo_root,
        })
    }

    pub fn is_initialized(&self) -> bool {
        self.config.is_some() && self.changeset_root.is_some() && self.config_path.is_some()
    }

    pub fn is_ci(&self) -> bool {
        env::var("GITHUB_ACTIONS").is_ok()
    }

    pub fn is_git_repo(&self) -> bool {
        self.repo_root.is_some()
    }

    pub fn has_package(&self, package: &str) -> bool {
        self.config
            .as_ref()
            .map(|c| c.packages.keys().any(|k| k == package))
            .unwrap_or(false)
    }
}
