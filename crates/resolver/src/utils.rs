use std::path::{Path, PathBuf};

use semver::Version;

use crate::changeset::BumpLevel;

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

pub fn list_files<F: Fn(&Path) -> bool>(path: &Path, filter: F) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_file() && filter(&path) {
            files.push(path);
        }
    }
    Ok(files)
}

pub fn bump_version(version: &str, level: BumpLevel) -> anyhow::Result<Version> {
    let mut version = semver::Version::parse(version)?;
    match level {
        BumpLevel::Minor => version.minor = version.minor + 1,
        BumpLevel::Major => version.major = version.major + 1,
        BumpLevel::Patch => version.patch = version.patch + 1,
    };
    Ok(version)
}
