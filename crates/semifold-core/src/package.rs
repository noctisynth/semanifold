use std::{fmt, str::FromStr};

use camino::Utf8PathBuf;
use semver::Version;
use serde::{Deserialize, Deserializer, Serialize};

use crate::Dependency;

/// Stable identity used by Semifold configuration and workspace graphs.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PackageId(String);

impl PackageId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Stable, serializable identity for a built-in or plugin-provided ecosystem.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EcosystemId(String);

impl EcosystemId {
    pub const MAX_LENGTH: usize = 128;

    /// Creates and validates an ecosystem identifier.
    ///
    /// Identifiers use lowercase ASCII segments separated by dots. Segments may contain digits and
    /// hyphens, but must start with a letter and end with a letter or digit.
    pub fn new(value: impl Into<String>) -> Result<Self, EcosystemIdError> {
        let value = value.into();
        validate_ecosystem_id(&value)?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_builtin(&self) -> bool {
        matches!(self.as_str(), "rust" | "node" | "python" | "cpp")
    }
}

impl fmt::Display for EcosystemId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for EcosystemId {
    type Err = EcosystemIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for EcosystemId {
    type Error = EcosystemIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for EcosystemId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum EcosystemIdError {
    #[error("ecosystem id must not be empty")]
    Empty,
    #[error("ecosystem id is {length} bytes; the maximum is {maximum}")]
    TooLong { length: usize, maximum: usize },
    #[error("invalid ecosystem id {value:?}; expected lowercase ASCII segments separated by dots")]
    InvalidFormat { value: String },
}

fn validate_ecosystem_id(value: &str) -> Result<(), EcosystemIdError> {
    if value.is_empty() {
        return Err(EcosystemIdError::Empty);
    }
    if value.len() > EcosystemId::MAX_LENGTH {
        return Err(EcosystemIdError::TooLong {
            length: value.len(),
            maximum: EcosystemId::MAX_LENGTH,
        });
    }

    let mut at_segment_start = true;
    let mut previous_was_hyphen = false;
    for character in value.chars() {
        if at_segment_start {
            if !character.is_ascii_lowercase() {
                return Err(EcosystemIdError::InvalidFormat {
                    value: value.to_string(),
                });
            }
            at_segment_start = false;
            previous_was_hyphen = false;
        } else if character == '.' {
            if previous_was_hyphen {
                return Err(EcosystemIdError::InvalidFormat {
                    value: value.to_string(),
                });
            }
            at_segment_start = true;
        } else if character == '-' {
            previous_was_hyphen = true;
        } else if character.is_ascii_lowercase() || character.is_ascii_digit() {
            previous_was_hyphen = false;
        } else {
            return Err(EcosystemIdError::InvalidFormat {
                value: value.to_string(),
            });
        }
    }

    if at_segment_start || previous_was_hyphen {
        return Err(EcosystemIdError::InvalidFormat {
            value: value.to_string(),
        });
    }
    Ok(())
}

/// Package ecosystem that owns manifest discovery and edit planning.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ecosystem {
    Rust,
    Node,
    Python,
    Cpp,
}

impl Ecosystem {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Node => "node",
            Self::Python => "python",
            Self::Cpp => "cpp",
        }
    }
}

impl From<Ecosystem> for EcosystemId {
    fn from(ecosystem: Ecosystem) -> Self {
        Self(ecosystem.id().to_string())
    }
}

/// Physical manifest location that owns a package's version value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VersionSource {
    PackageManifest,
    Shared { source: VersionSourceId },
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct VersionSourceId {
    pub manifest: Utf8PathBuf,
    pub field: String,
}

/// Immutable package data collected from an ecosystem manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageSnapshot {
    pub id: PackageId,
    pub manifest_name: String,
    pub version: Version,
    pub version_source: VersionSource,
    pub ecosystem: Ecosystem,
    pub path: Utf8PathBuf,
    pub publishable: bool,
    pub dependencies: Vec<Dependency>,
}

#[cfg(test)]
mod tests {
    use super::{Ecosystem, EcosystemId, EcosystemIdError};

    #[test]
    fn ecosystem_ids_validate_and_serialize_as_stable_strings() {
        let id = EcosystemId::new("com.example-game.engine2").unwrap();

        assert_eq!(id.as_str(), "com.example-game.engine2");
        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            r#""com.example-game.engine2""#
        );
        assert_eq!(
            serde_json::from_str::<EcosystemId>(r#""com.example-game.engine2""#).unwrap(),
            id
        );
    }

    #[test]
    fn ecosystem_ids_reject_non_canonical_values() {
        for value in [
            "",
            "Example",
            "example_kit",
            ".example",
            "example.",
            "example-.kit",
        ] {
            assert!(EcosystemId::new(value).is_err(), "{value}");
        }
        assert!(matches!(
            EcosystemId::new("x".repeat(EcosystemId::MAX_LENGTH + 1)),
            Err(EcosystemIdError::TooLong { .. })
        ));
    }

    #[test]
    fn built_in_ecosystems_keep_their_existing_serialized_ids() {
        let cases = [
            (Ecosystem::Rust, "rust"),
            (Ecosystem::Node, "node"),
            (Ecosystem::Python, "python"),
            (Ecosystem::Cpp, "cpp"),
        ];

        for (ecosystem, expected) in cases {
            let id = EcosystemId::from(ecosystem);
            assert_eq!(id.as_str(), expected);
            assert!(id.is_builtin());
        }
    }
}
