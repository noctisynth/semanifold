use std::{
    cmp::max,
    path::{Path, PathBuf},
};

use semver::Version;

use crate::{
    changeset::{BumpLevel, Changeset},
    config::{CommandConfig, VersionMode},
    error::ResolveError,
};

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

fn bump_prerelease(version: &mut Version, tag: &str) -> Result<(), ResolveError> {
    if version.pre.is_empty() {
        version.pre = semver::Prerelease::new(&format!("{tag}.0"))?;
    } else {
        let pre = version.pre.clone();
        let mut parts = pre.as_str().split('.').collect::<Vec<_>>();
        if let Some(idx) = parts.iter().position(|&s| s == tag) {
            if let Some(pre_patch) = parts.get(idx + 1) {
                let pre_patch =
                    pre_patch
                        .parse::<u64>()
                        .map_err(|e| ResolveError::InvalidVersion {
                            version: version.to_string(),
                            reason: e.to_string(),
                        })?;
                parts[idx + 1] = format!("{}", pre_patch + 1).leak();
            } else {
                parts.insert(idx + 1, "1");
            }
        } else {
            parts = vec![tag, "0"];
        }
        version.pre = semver::Prerelease::new(&parts.join("."))?;
    }
    Ok(())
}

pub fn bump_version<'a>(
    version: &'a mut Version,
    level: BumpLevel,
    mode: &VersionMode,
) -> Result<&'a mut Version, ResolveError> {
    match mode {
        VersionMode::Semantic => {
            if version.pre.is_empty() {
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
                    BumpLevel::Patch => {
                        version.patch += 1;
                    }
                    BumpLevel::Unchanged => {}
                }
            } else {
                // If the version is a pre-release, bumping semantic version resets pre-release
                version.pre = semver::Prerelease::EMPTY;
            }
        }
        VersionMode::PreRelease { tag } => {
            if tag.is_empty() {
                return Err(ResolveError::PreReleaseTagInvalid {
                    tag: tag.to_string(),
                    message: "Pre-release tag is empty".to_string(),
                });
            }
            bump_prerelease(version, tag)?;
        }
    }
    Ok(version)
}

pub fn get_bump_level(changesets: &[Changeset], package_name: &str) -> BumpLevel {
    let mut level = BumpLevel::Unchanged;
    for changeset in changesets {
        changeset.packages.iter().for_each(|package| {
            if package.name == package_name {
                level = max(level, package.level);
            }
        });
    }
    level
}

pub fn run_command(command: &CommandConfig, cwd: &Path) -> Result<(), ResolveError> {
    let mut cmd = std::process::Command::new(&command.command);
    if let Some(args) = &command.args {
        cmd.args(args);
    }
    cmd.current_dir(cwd);
    cmd.envs(&command.extra_env);
    cmd.stdout(command.stdout);
    cmd.stderr(command.stderr);
    let status = cmd.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(ResolveError::CommandError {
            command: command.command.clone(),
            status,
            code: status.code(),
        })
    }
}
