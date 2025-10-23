use std::path::{Path, PathBuf};

use semver::Version;

use crate::{changeset::BumpLevel, error::ResolveError};

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

pub fn list_files<F: Fn(&Path) -> bool>(
    path: &Path,
    filter: F,
) -> Result<Vec<PathBuf>, ResolveError> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_file() && filter(&path) {
            files.push(path);
        }
    }
    Ok(files)
}

pub fn bump_version(version: &str, level: BumpLevel) -> Result<Version, ResolveError> {
    let mut version =
        semver::Version::parse(version).map_err(|e| ResolveError::InvalidVersion {
            version: version.to_string(),
            reason: e.to_string(),
        })?;
    match level {
        BumpLevel::Major => {
            version.major += 1;
            version.minor = 0;
            version.patch = 0;
        }
        BumpLevel::Minor => {
            version.minor += 1;
            version.patch = 0;
        }
        BumpLevel::Patch => version.patch += 1,
    };
    Ok(version)
}
