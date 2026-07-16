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
        let mut parts: Vec<String> = pre.as_str().split('.').map(String::from).collect();
        if let Some(idx) = parts.iter().position(|s| s == tag) {
            if let Some(pre_patch) = parts.get(idx + 1) {
                let pre_patch =
                    pre_patch
                        .parse::<u64>()
                        .map_err(|e| ResolveError::InvalidVersion {
                            version: version.to_string(),
                            reason: e.to_string(),
                        })?;
                parts[idx + 1] = format!("{}", pre_patch + 1);
            } else {
                parts.insert(idx + 1, "1".to_string());
            }
        } else {
            parts = vec![tag.to_string(), "0".to_string()];
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

#[cfg(test)]
mod tests {
    use semver::Version;

    use crate::{
        changeset::{BumpLevel, Changeset},
        config::VersionMode,
        error::ResolveError,
    };

    use super::{bump_version, get_bump_level};

    #[test]
    fn bumps_semantic_versions() {
        let cases = [
            (BumpLevel::Major, "1.2.3", "2.0.0"),
            (BumpLevel::Minor, "1.2.3", "1.3.0"),
            (BumpLevel::Patch, "1.2.3", "1.2.4"),
            (BumpLevel::Unchanged, "1.2.3", "1.2.3"),
        ];

        for (level, current, expected) in cases {
            let mut version = Version::parse(current).unwrap();
            bump_version(&mut version, level, &VersionMode::Semantic).unwrap();
            assert_eq!(version, Version::parse(expected).unwrap());
        }
    }

    #[test]
    fn semantic_bump_finalizes_a_prerelease_without_incrementing() {
        let mut version = Version::parse("1.2.3-beta.4").unwrap();

        bump_version(&mut version, BumpLevel::Patch, &VersionMode::Semantic).unwrap();

        assert_eq!(version, Version::parse("1.2.3").unwrap());
    }

    #[test]
    fn prerelease_bump_initializes_increments_and_replaces_tags() {
        let mut version = Version::parse("1.2.3").unwrap();
        let beta = VersionMode::PreRelease {
            tag: "beta".to_string(),
        };

        bump_version(&mut version, BumpLevel::Patch, &beta).unwrap();
        assert_eq!(version, Version::parse("1.2.3-beta.0").unwrap());

        bump_version(&mut version, BumpLevel::Patch, &beta).unwrap();
        assert_eq!(version, Version::parse("1.2.3-beta.1").unwrap());

        bump_version(
            &mut version,
            BumpLevel::Patch,
            &VersionMode::PreRelease {
                tag: "rc".to_string(),
            },
        )
        .unwrap();
        assert_eq!(version, Version::parse("1.2.3-rc.0").unwrap());
    }

    #[test]
    fn prerelease_bump_rejects_an_empty_tag() {
        let mut version = Version::parse("1.2.3").unwrap();

        let error = bump_version(
            &mut version,
            BumpLevel::Patch,
            &VersionMode::PreRelease { tag: String::new() },
        )
        .unwrap_err();

        assert!(matches!(error, ResolveError::PreReleaseTagInvalid { .. }));
    }

    #[test]
    fn selects_the_highest_bump_for_the_requested_package() {
        let root = std::path::Path::new(".");
        let mut first = Changeset::new("first".to_string(), root);
        first.add_package("api".to_string(), BumpLevel::Patch, None);
        first.add_package("web".to_string(), BumpLevel::Major, None);

        let mut second = Changeset::new("second".to_string(), root);
        second.add_package("api".to_string(), BumpLevel::Minor, None);

        assert_eq!(get_bump_level(&[first, second], "api"), BumpLevel::Minor);
        assert_eq!(get_bump_level(&[], "api"), BumpLevel::Unchanged);
        assert_eq!(
            get_bump_level(&[Changeset::new("other".to_string(), root)], "unknown"),
            BumpLevel::Unchanged
        );
    }
}
