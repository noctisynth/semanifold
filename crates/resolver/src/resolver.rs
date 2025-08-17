use std::path::{Path, PathBuf};

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

pub fn find_at_parent(
    path_name: &str,
    starts_at: &Path,
    ends_at: Option<&Path>,
) -> Option<PathBuf> {
    let mut current_path = starts_at;
    loop {
        if ends_at.is_some() && current_path == ends_at.unwrap() {
            break None;
        } else {
            let config_path = current_path.join(path_name);
            if config_path.exists() {
                break Some(config_path);
            }
        }
        if let Some(parent_path) = current_path.parent() {
            current_path = parent_path;
        } else {
            break None;
        }
    }
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
