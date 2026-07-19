use camino::Utf8PathBuf;
use serde::Serialize;

use crate::PackageId;

/// Hash of the source content used while planning an edit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct FileHash(String);

impl FileHash {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Domain operation that produced a planned file edit.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EditSource {
    PackageVersion {
        package: PackageId,
    },
    DependencyVersion {
        package: PackageId,
        dependency: PackageId,
    },
    Changelog {
        package: PackageId,
    },
}

/// A validated, not-yet-applied file content replacement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileEdit {
    pub path: Utf8PathBuf,
    pub expected_hash: FileHash,
    pub new_content: String,
    pub source: EditSource,
}
