use std::{
    cell::RefCell,
    collections::HashMap,
    env,
    path::{Path, PathBuf},
};

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
    pub git_repo: Option<git2::Repository>,
    pub version_bumps: RefCell<HashMap<String, semver::Version>>,
    pub dry_run: bool,
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
        let git_repo = if let Some(repo_root) = &repo_root {
            git2::Repository::open(repo_root).ok()
        } else {
            None
        };

        Ok(Self {
            config,
            changeset_root,
            config_path,
            repo_root,
            repo_info,
            git_repo,
            ..Default::default()
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

    pub fn is_git_repo_clean(&self) -> bool {
        self.git_repo
            .as_ref()
            .and_then(|r| r.statuses(None).ok())
            .map(|s| s.iter().all(|s| s.status() == git2::Status::CURRENT))
            .unwrap_or(false)
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

    pub fn get_package_config(&self, package_config: &str) -> Option<&config::PackageConfig> {
        self.config.as_ref().unwrap().packages.get(package_config)
    }

    pub fn get_assets(
        &self,
        package_name: &str,
    ) -> Result<Vec<config::AssetConfig>, error::ResolveError> {
        let repo_root = self
            .repo_root
            .as_ref()
            .ok_or(error::ResolveError::GitError {
                message: "Git repository is not initialized".to_string(),
            })?;
        let assets = if let Some(pkg_cfg) = self.get_package_config(package_name) {
            pkg_cfg
                .assets
                .iter()
                .map(|p| match p {
                    config::Asset::Asset(asset_config) => config::AssetConfig {
                        path: repo_root.join(&asset_config.path),
                        name: asset_config.name.clone(),
                    },
                    config::Asset::String(path) => config::AssetConfig {
                        path: repo_root.join(path),
                        name: Path::new(path)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone()),
                    },
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(assets)
    }

    pub fn dry_run(&mut self, dry_run: bool) {
        self.dry_run = dry_run
    }
}
