use std::collections::{BTreeMap, BTreeSet};

use semver::Version;
use serde::Serialize;

use crate::{BumpLevel, ChangesetId, EcosystemId, FileEdit, PackageId, PlanWarning, ReleaseReason};

pub type VersionMap = BTreeMap<PackageId, Version>;

/// Planned version transition for one package that will be released.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PackageRelease {
    pub id: PackageId,
    pub ecosystem: EcosystemId,
    pub current_version: Version,
    pub next_version: Version,
    pub bump: BumpLevel,
    pub reasons: Vec<ReleaseReason>,
}

/// Immutable result of release-domain planning.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReleasePlan {
    packages: Vec<PackageRelease>,
    versions: VersionMap,
    order: Vec<PackageId>,
    consumed_changesets: Vec<ChangesetId>,
    warnings: Vec<PlanWarning>,
    file_edits: Vec<FileEdit>,
}

impl ReleasePlan {
    pub fn new(
        mut packages: Vec<PackageRelease>,
        versions: VersionMap,
        order: Vec<PackageId>,
        mut consumed_changesets: Vec<ChangesetId>,
        mut warnings: Vec<PlanWarning>,
        mut file_edits: Vec<FileEdit>,
    ) -> Result<Self, ReleasePlanError> {
        let mut package_ids = BTreeSet::new();
        for package in &mut packages {
            if !package_ids.insert(package.id.clone()) {
                return Err(ReleasePlanError::DuplicatePackage {
                    package: package.id.clone(),
                });
            }
            let planned =
                versions
                    .get(&package.id)
                    .ok_or_else(|| ReleasePlanError::MissingVersion {
                        package: package.id.clone(),
                    })?;
            if planned != &package.next_version {
                return Err(ReleasePlanError::VersionMismatch {
                    package: package.id.clone(),
                    release: package.next_version.clone(),
                    planned: planned.clone(),
                });
            }
            package.reasons.sort();
            if package
                .reasons
                .windows(2)
                .any(|reasons| matches!(reasons, [left, right] if left == right))
            {
                return Err(ReleasePlanError::DuplicateReason {
                    package: package.id.clone(),
                });
            }
        }
        packages.sort_by(|left, right| left.id.cmp(&right.id));

        let mut ordered_ids = BTreeSet::new();
        for package in &order {
            if !ordered_ids.insert(package.clone()) {
                return Err(ReleasePlanError::DuplicateOrderPackage {
                    package: package.clone(),
                });
            }
        }
        if ordered_ids != package_ids {
            return Err(ReleasePlanError::OrderDoesNotMatchPackages);
        }

        consumed_changesets.sort();
        if consumed_changesets
            .windows(2)
            .any(|changesets| matches!(changesets, [left, right] if left == right))
        {
            return Err(ReleasePlanError::DuplicateChangeset);
        }
        warnings.sort();
        file_edits.sort_by(|left, right| left.path.cmp(&right.path));
        if let Some(path) = file_edits.windows(2).find_map(|edits| match edits {
            [left, right] if left.path == right.path => Some(left.path.clone()),
            _ => None,
        }) {
            return Err(ReleasePlanError::DuplicateFileEdit { path });
        }

        Ok(Self {
            packages,
            versions,
            order,
            consumed_changesets,
            warnings,
            file_edits,
        })
    }

    #[must_use]
    pub fn packages(&self) -> &[PackageRelease] {
        &self.packages
    }

    #[must_use]
    pub fn package(&self, id: &PackageId) -> Option<&PackageRelease> {
        self.packages.iter().find(|package| &package.id == id)
    }

    #[must_use]
    pub fn versions(&self) -> &VersionMap {
        &self.versions
    }

    #[must_use]
    pub fn order(&self) -> &[PackageId] {
        &self.order
    }

    #[must_use]
    pub fn consumed_changesets(&self) -> &[ChangesetId] {
        &self.consumed_changesets
    }

    #[must_use]
    pub fn warnings(&self) -> &[PlanWarning] {
        &self.warnings
    }

    #[must_use]
    pub fn file_edits(&self) -> &[FileEdit] {
        &self.file_edits
    }

    /// Returns the same validated release plan with its planned file edits attached.
    pub fn with_file_edits(self, file_edits: Vec<FileEdit>) -> Result<Self, ReleasePlanError> {
        Self::new(
            self.packages,
            self.versions,
            self.order,
            self.consumed_changesets,
            self.warnings,
            file_edits,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ReleasePlanError {
    #[error("duplicate package release: {package}")]
    DuplicatePackage { package: PackageId },
    #[error("release package is missing from the complete version map: {package}")]
    MissingVersion { package: PackageId },
    #[error(
        "release version for {package} is {release}, but the complete version map contains {planned}"
    )]
    VersionMismatch {
        package: PackageId,
        release: Version,
        planned: Version,
    },
    #[error("duplicate release reason for package: {package}")]
    DuplicateReason { package: PackageId },
    #[error("duplicate package in release order: {package}")]
    DuplicateOrderPackage { package: PackageId },
    #[error("release order does not contain exactly the released packages")]
    OrderDoesNotMatchPackages,
    #[error("duplicate consumed changeset")]
    DuplicateChangeset,
    #[error("multiple planned edits target the same file: {path}")]
    DuplicateFileEdit { path: camino::Utf8PathBuf },
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;
    use crate::{EditSource, FileEditExpectation, FileHash};

    fn package_release(
        id: &str,
        current: (u64, u64, u64),
        next: (u64, u64, u64),
    ) -> PackageRelease {
        PackageRelease {
            id: PackageId::new(id),
            ecosystem: EcosystemId::RUST,
            current_version: Version::new(current.0, current.1, current.2),
            next_version: Version::new(next.0, next.1, next.2),
            bump: BumpLevel::Patch,
            reasons: vec![ReleaseReason::Changeset {
                changeset: ChangesetId::new(format!("{id}-change")),
            }],
        }
    }

    fn file_edit(package: &str, path: &str) -> FileEdit {
        FileEdit {
            path: Utf8PathBuf::from(path),
            expected: FileEditExpectation::Existing {
                hash: FileHash::from_bytes(format!("hash-{package}").as_bytes()),
            },
            new_content: format!("updated {package}"),
            source: EditSource::PackageVersion {
                package: PackageId::new(package),
            },
        }
    }

    #[test]
    fn canonicalizes_unordered_collections_for_stable_serialization() {
        let alpha = package_release("alpha", (1, 0, 0), (1, 0, 1));
        let beta = package_release("beta", (2, 0, 0), (2, 0, 1));
        let versions = BTreeMap::from([
            (PackageId::new("alpha"), Version::new(1, 0, 1)),
            (PackageId::new("beta"), Version::new(2, 0, 1)),
            (PackageId::new("unchanged"), Version::new(3, 0, 0)),
        ]);
        let first = ReleasePlan::new(
            vec![beta.clone(), alpha.clone()],
            versions.clone(),
            vec![PackageId::new("alpha"), PackageId::new("beta")],
            vec![
                ChangesetId::new("beta-change"),
                ChangesetId::new("alpha-change"),
            ],
            vec![
                PlanWarning::NonPatchBumpOnPrerelease {
                    package: PackageId::new("beta"),
                    requested: BumpLevel::Major,
                },
                PlanWarning::NonPatchBumpOnPrerelease {
                    package: PackageId::new("alpha"),
                    requested: BumpLevel::Minor,
                },
            ],
            vec![file_edit("beta", "b.toml"), file_edit("alpha", "a.toml")],
        )
        .unwrap();
        let second = ReleasePlan::new(
            vec![alpha, beta],
            versions,
            vec![PackageId::new("alpha"), PackageId::new("beta")],
            vec![
                ChangesetId::new("alpha-change"),
                ChangesetId::new("beta-change"),
            ],
            vec![
                PlanWarning::NonPatchBumpOnPrerelease {
                    package: PackageId::new("alpha"),
                    requested: BumpLevel::Minor,
                },
                PlanWarning::NonPatchBumpOnPrerelease {
                    package: PackageId::new("beta"),
                    requested: BumpLevel::Major,
                },
            ],
            vec![file_edit("alpha", "a.toml"), file_edit("beta", "b.toml")],
        )
        .unwrap();

        assert_eq!(first, second);
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
        assert_eq!(
            first.versions()[&PackageId::new("unchanged")],
            Version::new(3, 0, 0)
        );
    }

    #[test]
    fn rejects_release_versions_that_disagree_with_the_complete_map() {
        let package = package_release("core", (1, 0, 0), (1, 1, 0));
        let error = ReleasePlan::new(
            vec![package],
            BTreeMap::from([(PackageId::new("core"), Version::new(1, 0, 1))]),
            vec![PackageId::new("core")],
            vec![],
            vec![],
            vec![],
        )
        .unwrap_err();

        assert_eq!(
            error,
            ReleasePlanError::VersionMismatch {
                package: PackageId::new("core"),
                release: Version::new(1, 1, 0),
                planned: Version::new(1, 0, 1),
            }
        );
    }

    #[test]
    fn rejects_release_order_that_does_not_match_release_packages() {
        let package = package_release("core", (1, 0, 0), (1, 0, 1));
        let error = ReleasePlan::new(
            vec![package],
            BTreeMap::from([(PackageId::new("core"), Version::new(1, 0, 1))]),
            vec![PackageId::new("other")],
            vec![],
            vec![],
            vec![],
        )
        .unwrap_err();

        assert_eq!(error, ReleasePlanError::OrderDoesNotMatchPackages);
    }
}
