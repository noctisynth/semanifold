use serde::{Deserialize, Serialize};

use crate::{
    changeset::Changeset,
    config::{PackageConfig, ResolverConfig},
    context::Context,
    error::ResolveError,
    utils,
};
use core::fmt;
use std::path::{Path, PathBuf};

pub mod nodejs;
pub mod python;
pub mod rust;

#[derive(Serialize, Deserialize, Debug)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub private: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ResolverType {
    Rust,
    Nodejs,
    Python,
}

impl fmt::Display for ResolverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolverType::Rust => write!(f, "rust"),
            ResolverType::Nodejs => write!(f, "nodejs"),
            ResolverType::Python => write!(f, "python"),
        }
    }
}

pub trait Resolver {
    /// Resolve a package
    fn resolve(
        &mut self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<ResolvedPackage, ResolveError>;
    /// Resolve all packages
    fn resolve_all(&mut self, root: &Path) -> Result<Vec<ResolvedPackage>, ResolveError>;
    /// Bump version
    fn bump(
        &mut self,
        root: &Path,
        package: &ResolvedPackage,
        version: &semver::Version,
        dry_run: bool,
    ) -> Result<(), ResolveError>;
    /// Sort packages by their dependencies
    fn sort_packages(
        &mut self,
        root: &Path,
        packages: &mut Vec<(String, PackageConfig)>,
    ) -> Result<(), ResolveError>;
    /// Publish a package
    fn publish(
        &mut self,
        package: &ResolvedPackage,
        resolver_config: &ResolverConfig,
        dry_run: bool,
    ) -> Result<(), ResolveError>;
}

pub fn get_repo_root() -> Result<PathBuf, ResolveError> {
    let current_path = std::env::current_dir()?;
    let repo_root =
        utils::find_at_parent(".git", &current_path, None).ok_or(ResolveError::GitError {
            message: "No git repository found (or any of the parent directories): .git".to_string(),
        })?;
    Ok(repo_root)
}

pub fn get_changeset_path() -> Result<PathBuf, ResolveError> {
    let current_path = std::env::current_dir()?;

    let changeset_path = if let Ok(changeset_path) = std::env::var("CHANGESET_PATH") {
        PathBuf::from(changeset_path)
    } else {
        let changeset_dirs = [".changesets", ".changes"];
        changeset_dirs
            .iter()
            .find_map(|dir| utils::find_at_parent(dir, &current_path, None))
            .ok_or(ResolveError::FileOrDirNotFound { path: current_path })?
    };

    Ok(changeset_path)
}

pub fn get_changesets(ctx: &Context) -> Result<Vec<Changeset>, ResolveError> {
    if let Some(changeset_root) = ctx.changeset_root.as_ref() {
        let mut changesets = Vec::new();
        utils::list_files(changeset_root, |p| p.extension() == Some("md".as_ref()))?
            .into_iter()
            .try_fold(&mut changesets, |changesets, path| {
                changesets.push(Changeset::from_file(ctx, &path)?);
                log::debug!("Loaded changeset at: {}", &path.display());
                Ok::<_, ResolveError>(changesets)
            })?;
        Ok(changesets)
    } else {
        Ok(Vec::new())
    }
}
