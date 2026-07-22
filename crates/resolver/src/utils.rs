use std::{
    cmp::max,
    path::{Path, PathBuf},
};

use semver::Version;

use crate::{
    changeset::{BumpLevel, Changeset},
    config::{CommandConfig, ReleaseChannel},
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

fn bump_stable_base(version: &mut Version, level: BumpLevel) {
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
        BumpLevel::Unchanged => {}
    }
}

fn set_named_channel(
    version: &mut Version,
    channel: &str,
    sequence: u64,
) -> Result<(), ResolveError> {
    version.pre = semver::Prerelease::new(&format!("{channel}.{sequence}"))?;
    Ok(())
}

fn advance_named_channel(version: &mut Version, channel: &str) -> Result<(), ResolveError> {
    let prefix = format!("{channel}.");
    let sequence = version
        .pre
        .as_str()
        .strip_prefix(&prefix)
        .map(|value| {
            value
                .parse::<u64>()
                .map(|sequence| sequence + 1)
                .map_err(|error| ResolveError::InvalidVersion {
                    version: version.to_string(),
                    reason: error.to_string(),
                })
        })
        .transpose()?
        .unwrap_or(0);
    set_named_channel(version, channel, sequence)
}

pub fn bump_version<'a>(
    version: &'a mut Version,
    level: BumpLevel,
    channel: &ReleaseChannel,
) -> Result<&'a mut Version, ResolveError> {
    match channel {
        ReleaseChannel::Stable => {
            if version.pre.is_empty() {
                bump_stable_base(version, level);
            } else {
                // If the version is a pre-release, bumping semantic version resets pre-release
                version.pre = semver::Prerelease::EMPTY;
            }
        }
        ReleaseChannel::Named(name) => {
            if level != BumpLevel::Unchanged {
                if version.pre.is_empty() {
                    bump_stable_base(version, level);
                    set_named_channel(version, name, 0)?;
                } else {
                    advance_named_channel(version, name)?;
                }
            }
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

/// Replaces a root-object JSON string field without reformatting unrelated content.
pub fn replace_root_json_string_field(
    content: &str,
    field: &str,
    replacement: &str,
) -> Option<String> {
    let bytes = content.as_bytes();
    let mut index = skip_json_whitespace(bytes, 0);
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    index += 1;

    loop {
        index = skip_json_whitespace(bytes, index);
        if bytes.get(index) == Some(&b'}') {
            return None;
        }
        if bytes.get(index) != Some(&b'"') {
            return None;
        }

        let key_start = index;
        let key_end = scan_json_string(bytes, index)?;
        let key = serde_json::from_str::<String>(&content[key_start..key_end]).ok()?;
        index = skip_json_whitespace(bytes, key_end);
        if bytes.get(index) != Some(&b':') {
            return None;
        }
        index = skip_json_whitespace(bytes, index + 1);

        if key == field && bytes.get(index) == Some(&b'"') {
            let value_end = scan_json_string(bytes, index)?;
            let replacement = serde_json::to_string(replacement).ok()?;
            return Some(format!(
                "{}{}{}",
                &content[..index],
                replacement,
                &content[value_end..]
            ));
        }

        index = scan_json_value(bytes, index)?;
        index = skip_json_whitespace(bytes, index);
        match bytes.get(index) {
            Some(b',') => index += 1,
            Some(b'}') => return None,
            _ => return None,
        }
    }
}

fn skip_json_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while bytes
        .get(index)
        .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
    {
        index += 1;
    }
    index
}

fn scan_json_string(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'"') {
        return None;
    }

    let mut index = start + 1;
    while let Some(byte) = bytes.get(index) {
        match byte {
            b'\\' => index += 2,
            b'"' => return Some(index + 1),
            _ => index += 1,
        }
    }
    None
}

fn scan_json_value(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    let mut depth = 0usize;

    while let Some(byte) = bytes.get(index) {
        match byte {
            b'"' => index = scan_json_string(bytes, index)?,
            b'{' | b'[' => {
                depth += 1;
                index += 1;
            }
            b'}' | b']' if depth > 0 => {
                depth -= 1;
                index += 1;
            }
            b',' | b'}' if depth == 0 => return Some(index),
            _ => index += 1,
        }
    }
    None
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
        config::ReleaseChannel,
    };

    use super::{bump_version, get_bump_level, replace_root_json_string_field};

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
            bump_version(&mut version, level, &ReleaseChannel::Stable).unwrap();
            assert_eq!(version, Version::parse(expected).unwrap());
        }
    }

    #[test]
    fn semantic_bump_finalizes_a_prerelease_without_incrementing() {
        let mut version = Version::parse("1.2.3-beta.4").unwrap();

        bump_version(&mut version, BumpLevel::Patch, &ReleaseChannel::Stable).unwrap();

        assert_eq!(version, Version::parse("1.2.3").unwrap());
    }

    #[test]
    fn named_channel_sets_a_stable_base_then_advances_or_switches() {
        let mut version = Version::parse("1.2.3").unwrap();
        let beta = ReleaseChannel::Named("beta".to_string());

        bump_version(&mut version, BumpLevel::Major, &beta).unwrap();
        assert_eq!(version, Version::parse("2.0.0-beta.0").unwrap());

        bump_version(&mut version, BumpLevel::Major, &beta).unwrap();
        assert_eq!(version, Version::parse("2.0.0-beta.1").unwrap());

        bump_version(
            &mut version,
            BumpLevel::Patch,
            &ReleaseChannel::Named("rc".to_string()),
        )
        .unwrap();
        assert_eq!(version, Version::parse("2.0.0-rc.0").unwrap());
    }

    #[test]
    fn unchanged_named_channel_does_not_advance() {
        let mut version = Version::parse("1.2.3").unwrap();

        bump_version(
            &mut version,
            BumpLevel::Unchanged,
            &ReleaseChannel::Named("beta".to_string()),
        )
        .unwrap();

        assert_eq!(version, Version::parse("1.2.3").unwrap());
    }

    #[test]
    fn changeset_tags_do_not_change_the_release_channel() {
        let root = std::path::Path::new(".");
        let mut changeset = Changeset::new("feature".to_string(), root);
        changeset.add_package(
            "api".to_string(),
            BumpLevel::Patch,
            Some("breaking-change".to_string()),
        );
        let mut version = Version::parse("1.2.3").unwrap();

        bump_version(
            &mut version,
            get_bump_level(&[changeset], "api"),
            &ReleaseChannel::Named("beta".to_string()),
        )
        .unwrap();

        assert_eq!(version, Version::parse("1.2.4-beta.0").unwrap());
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

    #[test]
    fn replaces_only_the_root_json_string_field_without_reformatting() {
        let content = concat!(
            "{\n",
            "  \"metadata\": { \"version\": \"unchanged\" },\n",
            "  \"version\" : \"1.0.0\",\n",
            "  \"custom\": [1, 2]\n",
            "}\n"
        );

        assert_eq!(
            replace_root_json_string_field(content, "version", "1.0.1"),
            Some(
                concat!(
                    "{\n",
                    "  \"metadata\": { \"version\": \"unchanged\" },\n",
                    "  \"version\" : \"1.0.1\",\n",
                    "  \"custom\": [1, 2]\n",
                    "}\n"
                )
                .to_string()
            )
        );
    }
}
