use camino::Utf8PathBuf;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{PackageId, VersionSourceId};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SharedVersionEdit {
    pub source: VersionSourceId,
    pub packages: Vec<PackageId>,
}

/// Hash of the source content used while planning an edit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct FileHash(String);

impl FileHash {
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
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
    WorkspaceDependencies {
        dependencies: Vec<PackageId>,
    },
    WorkspaceManifest {
        shared_versions: Vec<SharedVersionEdit>,
        dependencies: Vec<PackageId>,
    },
    Changelog {
        package: PackageId,
    },
}

/// Required target state when a planned edit is applied.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileEditExpectation {
    Existing { hash: FileHash },
    Missing,
}

/// A validated, not-yet-applied file content replacement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileEdit {
    pub path: Utf8PathBuf,
    pub expected: FileEditExpectation,
    pub new_content: String,
    pub source: EditSource,
}

#[cfg(test)]
mod tests {
    use super::FileHash;

    #[test]
    fn hashes_source_bytes_with_sha256() {
        assert_eq!(
            FileHash::from_bytes(b"semifold").as_str(),
            "acfa94237c0f2abcae06590ebe6bb12455e24f07a9608a2418d618b540aee4e0"
        );
    }
}
