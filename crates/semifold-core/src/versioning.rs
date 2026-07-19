use semver::{Prerelease, Version};
use serde::Serialize;

use crate::BumpLevel;

/// Release channel selected for a package.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum ReleaseChannel {
    #[default]
    Stable,
    Named(String),
}

/// Computes one package version without reading configuration or manifests.
pub fn bump_version(
    current: &Version,
    bump: BumpLevel,
    channel: &ReleaseChannel,
) -> Result<Version, VersioningError> {
    let mut next = current.clone();
    if bump == BumpLevel::Unchanged {
        return Ok(next);
    }

    match channel {
        ReleaseChannel::Stable => {
            if next.pre.is_empty() {
                bump_stable_base(&mut next, bump);
            } else {
                next.pre = Prerelease::EMPTY;
            }
        }
        ReleaseChannel::Named(name) => {
            if next.pre.is_empty() {
                bump_stable_base(&mut next, bump);
                set_named_channel(&mut next, name, 0)?;
            } else {
                advance_named_channel(&mut next, name)?;
            }
        }
    }
    Ok(next)
}

fn bump_stable_base(version: &mut Version, bump: BumpLevel) {
    match bump {
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
) -> Result<(), VersioningError> {
    version.pre = Prerelease::new(&format!("{channel}.{sequence}")).map_err(|error| {
        VersioningError::InvalidChannel {
            channel: channel.to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(())
}

fn advance_named_channel(version: &mut Version, channel: &str) -> Result<(), VersioningError> {
    let prefix = format!("{channel}.");
    let sequence = version
        .pre
        .as_str()
        .strip_prefix(&prefix)
        .map(|value| {
            value
                .parse::<u64>()
                .map(|sequence| sequence + 1)
                .map_err(|error| VersioningError::InvalidSequence {
                    version: version.clone(),
                    channel: channel.to_string(),
                    reason: error.to_string(),
                })
        })
        .transpose()?
        .unwrap_or(0);
    set_named_channel(version, channel, sequence)
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum VersioningError {
    #[error("invalid release channel {channel}: {reason}")]
    InvalidChannel { channel: String, reason: String },
    #[error("invalid {channel} sequence in version {version}: {reason}")]
    InvalidSequence {
        version: Version,
        channel: String,
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(value: &str) -> Version {
        Version::parse(value).unwrap()
    }

    #[test]
    fn computes_stable_and_named_channel_transitions() {
        assert_eq!(
            bump_version(&version("1.2.3"), BumpLevel::Minor, &ReleaseChannel::Stable).unwrap(),
            version("1.3.0")
        );
        assert_eq!(
            bump_version(
                &version("1.2.3"),
                BumpLevel::Minor,
                &ReleaseChannel::Named("alpha".to_string())
            )
            .unwrap(),
            version("1.3.0-alpha.0")
        );
        assert_eq!(
            bump_version(
                &version("1.3.0-alpha.2"),
                BumpLevel::Major,
                &ReleaseChannel::Named("alpha".to_string())
            )
            .unwrap(),
            version("1.3.0-alpha.3")
        );
        assert_eq!(
            bump_version(
                &version("1.3.0-alpha.2"),
                BumpLevel::Patch,
                &ReleaseChannel::Named("beta".to_string())
            )
            .unwrap(),
            version("1.3.0-beta.0")
        );
        assert_eq!(
            bump_version(
                &version("1.3.0-alpha.2"),
                BumpLevel::Patch,
                &ReleaseChannel::Stable
            )
            .unwrap(),
            version("1.3.0")
        );
    }

    #[test]
    fn unchanged_never_advances_a_channel() {
        let current = version("1.3.0-alpha.2");
        assert_eq!(
            bump_version(
                &current,
                BumpLevel::Unchanged,
                &ReleaseChannel::Named("alpha".to_string())
            )
            .unwrap(),
            current
        );
    }
}
