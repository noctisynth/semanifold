use std::path::{Path, PathBuf};

use crate::{
    changeset::Changeset,
    utils::{self, find_at_parent},
};

pub mod rust;

pub struct ResolvedPackage {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
}

pub trait Resolver {
    fn resolve(&self) -> anyhow::Result<ResolvedPackage>;
    fn bump(&self, version: &str) -> anyhow::Result<()>;
}

pub fn get_changeset_path() -> anyhow::Result<PathBuf> {
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

    Ok(changeset_path)
}

pub fn get_changesets(path: &Path) -> anyhow::Result<Vec<Changeset>> {
    let mut changesets = Vec::new();
    utils::list_files(path, |p| p.extension() == Some("md".as_ref()))?
        .into_iter()
        .try_fold(&mut changesets, |changesets, path| {
            changesets.push(Changeset::from_file(&path)?);
            Ok::<_, anyhow::Error>(changesets)
        })?;
    Ok(changesets)
}
