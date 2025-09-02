use crate::{
    changeset::{BumpLevel, Changeset},
    config::PackageConfig,
    error::ResolveError,
    utils,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub mod rust;
pub mod toml_serializer;
pub struct ResolvedPackage {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigPackage {
    pub path: PathBuf,
}

//
#[derive(Serialize, Deserialize)]
pub struct ChangesConfig {
    #[serde(rename = "packages", serialize_with = "toml_serializer::serialize")]
    packages: BTreeMap<String, ConfigPackage>,
    tags: BTreeMap<String, String>,
}

pub trait Resolver {
    /// Resolve a package
    fn resolve(&mut self, pkg_config: &PackageConfig) -> Result<ResolvedPackage, ResolveError>;
    /// Resolve all packages
    fn resolve_all(&mut self) -> Result<Vec<ResolvedPackage>, ResolveError>;
    /// Bump version
    fn bump(&mut self, package: &ResolvedPackage, level: BumpLevel) -> Result<(), ResolveError>;
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

pub fn get_changesets(path: &Path) -> Result<Vec<Changeset>, ResolveError> {
    let mut changesets = Vec::new();
    utils::list_files(path, |p| p.extension() == Some("md".as_ref()))?
        .into_iter()
        .try_fold(&mut changesets, |changesets, path| {
            changesets.push(Changeset::from_file(&path)?);
            Ok::<_, ResolveError>(changesets)
        })?;
    Ok(changesets)
}
