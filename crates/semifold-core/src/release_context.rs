use std::collections::BTreeMap;

use semver::Version;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    BumpLevel, ChangesetId, EcosystemId, PackageId, PackageSnapshot, ReleasePlan, ReleaseReason,
};

/// Immutable, serializable facts shared by one workspace release.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReleaseContext {
    pub plan: ReleasePlanContext,
    pub repository: Option<RepositoryContext>,
    pub ci: Option<CiContext>,
}

impl ReleaseContext {
    #[must_use]
    pub fn from_plan(plan: &ReleasePlan) -> Self {
        Self {
            plan: ReleasePlanContext::from(plan),
            repository: None,
            ci: None,
        }
    }

    #[must_use]
    pub fn with_repository(mut self, repository: RepositoryContext) -> Self {
        self.repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_ci(mut self, ci: CiContext) -> Self {
        self.ci = Some(ci);
        self
    }
}

/// Template-safe projection of a validated release plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReleasePlanContext {
    pub packages: BTreeMap<PackageId, PackageReleaseContext>,
    pub changesets: Vec<ChangesetId>,
    pub common_version: Option<Version>,
    pub fingerprint: String,
}

impl From<&ReleasePlan> for ReleasePlanContext {
    fn from(plan: &ReleasePlan) -> Self {
        let packages = plan
            .packages()
            .iter()
            .map(|package| {
                (
                    package.id.clone(),
                    PackageReleaseContext {
                        id: package.id.clone(),
                        ecosystem: package.ecosystem.clone(),
                        current_version: package.current_version.clone(),
                        next_version: package.next_version.clone(),
                        bump: package.bump,
                        reasons: package
                            .reasons
                            .iter()
                            .map(ReleaseReasonContext::from)
                            .collect(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let changesets = plan.consumed_changesets().to_vec();
        let common_version = common_version(&packages);
        let fingerprint = plan_fingerprint(&packages, &changesets);

        Self {
            packages,
            changesets,
            common_version,
            fingerprint,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PackageReleaseContext {
    pub id: PackageId,
    pub ecosystem: EcosystemId,
    pub current_version: Version,
    pub next_version: Version,
    pub bump: BumpLevel,
    pub reasons: Vec<ReleaseReasonContext>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReleaseReasonContext {
    Changeset {
        changeset: ChangesetId,
    },
    DependencyPropagation {
        dependency: PackageId,
        next_version: Version,
    },
    SharedVersionPropagation {
        source: crate::VersionSourceId,
    },
}

impl From<&ReleaseReason> for ReleaseReasonContext {
    fn from(reason: &ReleaseReason) -> Self {
        match reason {
            ReleaseReason::Changeset { changeset } => Self::Changeset {
                changeset: changeset.clone(),
            },
            ReleaseReason::DependencyPropagation {
                dependency,
                next_version,
            } => Self::DependencyPropagation {
                dependency: dependency.clone(),
                next_version: next_version.clone(),
            },
            ReleaseReason::SharedVersionPropagation { source } => Self::SharedVersionPropagation {
                source: source.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepositoryContext {
    pub host: String,
    pub owner: String,
    pub name: String,
    pub web_url: String,
    pub commit: Option<CommitContext>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CommitContext {
    pub sha: String,
    pub short_sha: String,
    pub author: Option<String>,
    pub web_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PullRequestContext {
    pub number: u64,
    pub author: Option<String>,
    pub web_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CiContext {
    pub provider: CiProvider,
    pub run_id: Option<String>,
    pub run_url: Option<String>,
    pub ref_name: Option<String>,
}

/// Stable provider name without coupling the domain to one CI vendor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CiProvider(String);

impl CiProvider {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReleasePackageContext<'release> {
    pub release: &'release ReleaseContext,
    pub package: ReleasePackageTemplateContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReleasePackageTemplateContext {
    pub id: PackageId,
    pub name: String,
    pub ecosystem: EcosystemId,
    pub current_version: Version,
    pub next_version: Version,
    pub version: Version,
    pub tag: String,
    pub path: camino::Utf8PathBuf,
    pub private: bool,
}

impl<'release> ReleasePackageContext<'release> {
    pub fn from_snapshot(
        release: &'release ReleaseContext,
        snapshot: &PackageSnapshot,
    ) -> Result<Self, ReleasePackageContextError> {
        let planned = release.plan.packages.get(&snapshot.id).ok_or_else(|| {
            ReleasePackageContextError::PackageNotReleased {
                package: snapshot.id.clone(),
            }
        })?;
        if snapshot.version != planned.current_version {
            return Err(ReleasePackageContextError::CurrentVersionMismatch {
                package: snapshot.id.clone(),
                snapshot: snapshot.version.clone(),
                planned: planned.current_version.clone(),
            });
        }
        if snapshot.ecosystem != planned.ecosystem {
            return Err(ReleasePackageContextError::EcosystemMismatch {
                package: snapshot.id.clone(),
                snapshot: snapshot.ecosystem.clone(),
                planned: planned.ecosystem.clone(),
            });
        }
        let next_version = planned.next_version.clone();
        Ok(Self {
            release,
            package: ReleasePackageTemplateContext {
                id: snapshot.id.clone(),
                name: snapshot.manifest_name.clone(),
                ecosystem: snapshot.ecosystem.clone(),
                current_version: snapshot.version.clone(),
                version: next_version.clone(),
                tag: format!("{}-v{next_version}", snapshot.manifest_name),
                next_version,
                path: snapshot.path.clone(),
                private: !snapshot.publishable,
            },
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ReleasePackageContextError {
    #[error("package is not part of the release plan: {package}")]
    PackageNotReleased { package: PackageId },
    #[error(
        "package {package} has current version {snapshot}, but the release plan expects {planned}"
    )]
    CurrentVersionMismatch {
        package: PackageId,
        snapshot: Version,
        planned: Version,
    },
    #[error(
        "package {package} has ecosystem {snapshot_name}, but the release plan expects {planned_name}",
        snapshot_name = .snapshot.display_name(),
        planned_name = .planned.display_name()
    )]
    EcosystemMismatch {
        package: PackageId,
        snapshot: EcosystemId,
        planned: EcosystemId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChangesetContext {
    pub id: ChangesetId,
    pub summary: String,
    pub summary_paragraphs: Vec<Vec<String>>,
    pub commit: Option<CommitContext>,
    pub pull_request: Option<PullRequestContext>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PackageChangesetContext {
    pub changeset: ChangesetContext,
    pub section: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DependencyUpdateContext {
    pub package: PackageId,
    pub next_version: Version,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChangelogContext<'release> {
    pub package: ReleasePackageContext<'release>,
    pub changesets: Vec<PackageChangesetContext>,
    pub dependency_updates: Vec<DependencyUpdateContext>,
}

fn common_version(packages: &BTreeMap<PackageId, PackageReleaseContext>) -> Option<Version> {
    let mut versions = packages.values().map(|package| &package.next_version);
    let first = versions.next()?;
    versions
        .all(|version| version == first)
        .then(|| first.clone())
}

fn plan_fingerprint(
    packages: &BTreeMap<PackageId, PackageReleaseContext>,
    changesets: &[ChangesetId],
) -> String {
    let mut hasher = Sha256::new();
    for (id, package) in packages {
        hash_field(&mut hasher, b"package", id.as_str());
        hash_field(&mut hasher, b"version", &package.next_version.to_string());
    }
    for changeset in changesets {
        hash_field(&mut hasher, b"changeset", changeset.as_str());
    }
    let digest = hasher.finalize();
    digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hash_field(hasher: &mut Sha256, kind: &[u8], value: &str) {
    hasher.update(kind);
    hasher.update([0]);
    hasher.update(value.len().to_string().as_bytes());
    hasher.update(b":");
    hasher.update(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PackageRelease, VersionMap, VersionSource};

    fn package(id: &str, next: Version) -> PackageRelease {
        PackageRelease {
            id: PackageId::new(id),
            ecosystem: EcosystemId::RUST,
            current_version: Version::new(1, 0, 0),
            next_version: next,
            bump: BumpLevel::Minor,
            reasons: vec![ReleaseReason::Changeset {
                changeset: ChangesetId::new(format!("{id}-change")),
            }],
        }
    }

    fn plan(packages: Vec<PackageRelease>, changesets: Vec<&str>) -> ReleasePlan {
        let versions = packages
            .iter()
            .map(|package| (package.id.clone(), package.next_version.clone()))
            .collect::<VersionMap>();
        let order = packages.iter().map(|package| package.id.clone()).collect();
        ReleasePlan::new(
            packages,
            versions,
            order,
            changesets.into_iter().map(ChangesetId::new).collect(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
    }

    #[test]
    fn single_package_has_common_version_and_stable_projection() {
        let plan = plan(
            vec![package("core", Version::new(1, 1, 0))],
            vec!["core-change"],
        );
        let context = ReleasePlanContext::from(&plan);

        assert_eq!(context.common_version, Some(Version::new(1, 1, 0)));
        assert_eq!(context.changesets, vec![ChangesetId::new("core-change")]);
        assert_eq!(
            context.packages.keys().next(),
            Some(&PackageId::new("core"))
        );
        assert_eq!(context.fingerprint.len(), 12);
        assert!(
            context
                .fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
    }

    #[test]
    fn multiple_packages_only_have_common_version_when_versions_match() {
        let matching = plan(
            vec![
                package("b", Version::new(2, 0, 0)),
                package("a", Version::new(2, 0, 0)),
            ],
            vec!["b-change", "a-change"],
        );
        let different = plan(
            vec![
                package("a", Version::new(2, 0, 0)),
                package("b", Version::new(1, 1, 0)),
            ],
            vec!["a-change", "b-change"],
        );

        assert_eq!(
            ReleasePlanContext::from(&matching).common_version,
            Some(Version::new(2, 0, 0))
        );
        assert_eq!(ReleasePlanContext::from(&different).common_version, None);
    }

    #[test]
    fn empty_plan_has_no_common_version() {
        let context = ReleasePlanContext::from(&plan(Vec::new(), Vec::new()));

        assert_eq!(context.common_version, None);
        assert!(context.packages.is_empty());
    }

    #[test]
    fn fingerprint_is_independent_of_input_order() {
        let first = plan(
            vec![
                package("b", Version::new(1, 2, 0)),
                package("a", Version::new(2, 0, 0)),
            ],
            vec!["z-change", "a-change"],
        );
        let second = plan(
            vec![
                package("a", Version::new(2, 0, 0)),
                package("b", Version::new(1, 2, 0)),
            ],
            vec!["a-change", "z-change"],
        );

        assert_eq!(
            ReleasePlanContext::from(&first).fingerprint,
            ReleasePlanContext::from(&second).fingerprint
        );
    }

    #[test]
    fn fingerprint_changes_with_version_or_changeset_identity() {
        let baseline = ReleasePlanContext::from(&plan(
            vec![package("core", Version::new(1, 1, 0))],
            vec!["change-a"],
        ));
        let version_changed = ReleasePlanContext::from(&plan(
            vec![package("core", Version::new(1, 2, 0))],
            vec!["change-a"],
        ));
        let changeset_changed = ReleasePlanContext::from(&plan(
            vec![package("core", Version::new(1, 1, 0))],
            vec!["change-b"],
        ));

        assert_ne!(baseline.fingerprint, version_changed.fingerprint);
        assert_ne!(baseline.fingerprint, changeset_changed.fingerprint);
    }

    #[test]
    fn release_package_context_projects_manifest_and_planned_facts() {
        let plan = plan(
            vec![package("configured-id", Version::new(1, 1, 0))],
            vec!["release-change"],
        );
        let release = ReleaseContext::from_plan(&plan);
        let snapshot = PackageSnapshot {
            id: PackageId::new("configured-id"),
            manifest_name: "manifest-name".to_string(),
            version: Version::new(1, 0, 0),
            version_source: VersionSource::PackageManifest,
            ecosystem: EcosystemId::RUST,
            path: "crates/package".into(),
            publishable: false,
            dependencies: Vec::new(),
        };

        let context = ReleasePackageContext::from_snapshot(&release, &snapshot).unwrap();

        assert_eq!(context.package.id, PackageId::new("configured-id"));
        assert_eq!(context.package.name, "manifest-name");
        assert_eq!(context.package.current_version, Version::new(1, 0, 0));
        assert_eq!(context.package.next_version, Version::new(1, 1, 0));
        assert_eq!(context.package.version, Version::new(1, 1, 0));
        assert_eq!(context.package.tag, "manifest-name-v1.1.0");
        assert!(context.package.private);
    }

    #[test]
    fn release_package_context_rejects_non_released_or_changed_snapshots() {
        let plan = plan(
            vec![package("configured-id", Version::new(1, 1, 0))],
            vec!["release-change"],
        );
        let release = ReleaseContext::from_plan(&plan);
        let mut snapshot = PackageSnapshot {
            id: PackageId::new("missing"),
            manifest_name: "missing".to_string(),
            version: Version::new(1, 0, 0),
            version_source: VersionSource::PackageManifest,
            ecosystem: EcosystemId::RUST,
            path: "crates/missing".into(),
            publishable: true,
            dependencies: Vec::new(),
        };

        assert!(matches!(
            ReleasePackageContext::from_snapshot(&release, &snapshot),
            Err(ReleasePackageContextError::PackageNotReleased { .. })
        ));

        snapshot.id = PackageId::new("configured-id");
        snapshot.version = Version::new(1, 0, 1);
        assert!(matches!(
            ReleasePackageContext::from_snapshot(&release, &snapshot),
            Err(ReleasePackageContextError::CurrentVersionMismatch { .. })
        ));
    }
}
