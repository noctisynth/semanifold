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

use semifold_core::DependencyKind;

pub mod cpp;
pub mod nodejs;
pub mod python;
pub mod rust;

#[derive(Serialize, Deserialize, Debug)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: semver::Version,
    pub path: PathBuf,
    pub private: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedDependency {
    pub manifest_name: String,
    pub kind: DependencyKind,
    pub requirement: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "snake_case")]
pub enum ResolverType {
    Rust,
    Nodejs,
    Python,
    Cpp,
}

impl fmt::Display for ResolverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolverType::Rust => write!(f, "Rust"),
            ResolverType::Nodejs => write!(f, "Nodejs"),
            ResolverType::Python => write!(f, "Python"),
            ResolverType::Cpp => write!(f, "Cpp"),
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
    /// Inspect manifest dependencies without deciding whether they are internal.
    fn dependencies(
        &mut self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<Vec<ResolvedDependency>, ResolveError>;
    /// Publish a package
    fn publish(
        &mut self,
        package: &ResolvedPackage,
        resolver_config: &ResolverConfig,
        dry_run: bool,
    ) -> Result<(), ResolveError>;
}

pub fn create_resolver(resolver_type: ResolverType) -> Box<dyn Resolver> {
    match resolver_type {
        ResolverType::Rust => Box::new(rust::RustResolver),
        ResolverType::Nodejs => Box::new(nodejs::NodejsResolver),
        ResolverType::Python => Box::new(python::PythonResolver),
        ResolverType::Cpp => Box::new(cpp::CppResolver),
    }
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
                log::debug!("Loaded changeset at: {}", path.display());
                Ok::<_, ResolveError>(changesets)
            })?;
        Ok(changesets)
    } else {
        Ok(Vec::new())
    }
}
