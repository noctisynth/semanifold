use std::fmt;

use camino::Utf8PathBuf;
use semver::Version;
use serde::{Deserialize, Serialize};

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

/// Package ecosystem that owns manifest discovery and edit planning.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ecosystem {
    Rust,
    Node,
    Python,
    Cpp,
}

/// Immutable package data collected from an ecosystem manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageSnapshot {
    pub id: PackageId,
    pub manifest_name: String,
    pub version: Version,
    pub ecosystem: Ecosystem,
    pub path: Utf8PathBuf,
    pub publishable: bool,
    pub dependencies: Vec<Dependency>,
}
