use std::fmt;

use semver::Version;
use serde::Serialize;

use crate::{PackageId, VersionSourceId};

/// Stable identity of one changeset file without its storage location.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ChangesetId(String);

impl ChangesetId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChangesetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Semantic version change requested for one package.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BumpLevel {
    Unchanged,
    Patch,
    Minor,
    Major,
}

impl fmt::Display for BumpLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unchanged => "unchanged",
            Self::Patch => "patch",
            Self::Minor => "minor",
            Self::Major => "major",
        })
    }
}

/// Why a package is included in a release.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReleaseReason {
    Changeset {
        changeset: ChangesetId,
    },
    DependencyPropagation {
        dependency: PackageId,
        next_version: Version,
    },
    SharedVersionPropagation {
        source: VersionSourceId,
    },
}

/// Non-fatal domain condition to render alongside a release plan.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanWarning {
    NonPatchBumpOnPrerelease {
        package: PackageId,
        requested: BumpLevel,
    },
}
