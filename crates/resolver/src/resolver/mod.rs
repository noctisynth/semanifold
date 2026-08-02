use serde::{Deserialize, Serialize};

use crate::{changeset::Changeset, config::Config, error::ResolveError, utils};
use core::fmt;
use std::path::{Path, PathBuf};

pub mod cpp;
pub mod nodejs;
pub mod python;
pub mod rust;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
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

pub fn get_changesets(
    changeset_root: &Path,
    config: &Config,
) -> Result<Vec<Changeset>, ResolveError> {
    let mut changesets = Vec::new();
    utils::list_files(changeset_root, |p| p.extension() == Some("md".as_ref()))?
        .into_iter()
        .try_fold(&mut changesets, |changesets, path| {
            changesets.push(Changeset::from_file(config, &path)?);
            log::debug!("Loaded changeset at: {}", path.display());
            Ok::<_, ResolveError>(changesets)
        })?;
    Ok(changesets)
}
