use std::{env, path::PathBuf};

use crate::{config, error, resolver};

#[derive(Debug)]
pub struct RepoInfo {
    pub owner: String,
    pub repo_name: String,
}

#[derive(Default)]
pub struct Context {
    pub config: Option<config::Config>,
    pub changeset_root: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub repo_root: Option<PathBuf>,
    pub repo_info: Option<RepoInfo>,
}

impl Context {
    pub fn create() -> Result<Self, error::ResolveError> {
        let changeset_root = resolver::get_changeset_path().ok();
        let config_path = if let Some(changeset_root) = &changeset_root {
            config::get_config_path(changeset_root).ok()
        } else {
            None
        };
        let config = if let Some(config_path) = &config_path {
            Some(config::load_config(config_path)?)
        } else {
            None
        };
        let repo_root = resolver::get_repo_root()
            .ok()
            .and_then(|path| path.parent().map(|p| p.to_path_buf()));
        let repo_info = std::env::var("GITHUB_REPOSITORY").ok().and_then(|repo| {
            repo.split_once('/').map(|(owner, repo_name)| RepoInfo {
                owner: owner.to_string(),
                repo_name: repo_name.to_string(),
            })
        });

        Ok(Self {
            config,
            changeset_root,
            config_path,
            repo_root,
            repo_info,
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
            .is_some_and(|c| c.packages.contains_key(package))
    }

    pub fn create_resolver(
        &self,
        resolver_type: resolver::ResolverType,
    ) -> Box<dyn resolver::Resolver> {
        match resolver_type {
            resolver::ResolverType::Rust => Box::new(resolver::rust::RustResolver),
            resolver::ResolverType::Nodejs => Box::new(resolver::nodejs::NodejsResolver),
            resolver::ResolverType::Python => Box::new(resolver::python::PythonResolver),
        }
    }

    pub fn get_resolver_config(
        &self,
        resolver_type: resolver::ResolverType,
    ) -> Option<&config::ResolverConfig> {
        self.config
            .as_ref()
            .and_then(|c| c.resolver.get(&resolver_type))
    }

    pub fn get_resolvers(&self) -> Vec<resolver::ResolverType> {
        self.config
            .as_ref()
            .map(|c| c.resolver.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_packages(&self) -> Vec<(&String, &config::PackageConfig)> {
        self.config
            .as_ref()
            .map(|c| c.packages.iter().collect())
            .unwrap_or_default()
    }
}
