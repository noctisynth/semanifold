use std::collections::{BTreeMap, BTreeSet};

use semver::VersionReq;

use crate::{
    BumpLevel, ChangesetId, PackageId, PackageRelease, PlanWarning, ReleaseChannel, ReleasePlan,
    ReleasePlanError, ReleaseReason, VersionMap, VersioningError, WorkspaceGraph,
    WorkspaceGraphError, bump_version,
};

/// Parsed changeset facts required by release planning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangesetInput {
    pub id: ChangesetId,
    pub releases: BTreeMap<PackageId, BumpLevel>,
}

/// Per-package channel and adapter-approved dependency propagation rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageReleasePolicy {
    pub channel: ReleaseChannel,
    /// One-shot override for the stable base when first entering a named channel.
    pub channel_bump: Option<BumpLevel>,
    pub propagating_dependencies: BTreeMap<PackageId, Option<VersionReq>>,
}

pub type ReleasePolicies = BTreeMap<PackageId, PackageReleasePolicy>;

/// Pure release-domain planner.
pub struct ReleasePlanner;

impl ReleasePlanner {
    pub fn plan(
        graph: &WorkspaceGraph,
        changesets: &[ChangesetInput],
        policies: &ReleasePolicies,
    ) -> Result<ReleasePlan, ReleasePlannerError> {
        Self::validate_policies(graph, policies)?;

        let mut levels = graph
            .packages()
            .map(|package| (package.id.clone(), BumpLevel::Unchanged))
            .collect::<BTreeMap<_, _>>();
        let mut reasons = BTreeMap::<PackageId, BTreeSet<ReleaseReason>>::new();
        for changeset in changesets {
            for (package, bump) in &changeset.releases {
                let level = levels.get_mut(package).ok_or_else(|| {
                    ReleasePlannerError::UnknownChangesetPackage {
                        changeset: changeset.id.clone(),
                        package: package.clone(),
                    }
                })?;
                *level = (*level).max(*bump);
                if *bump != BumpLevel::Unchanged {
                    reasons
                        .entry(package.clone())
                        .or_default()
                        .insert(ReleaseReason::Changeset {
                            changeset: changeset.id.clone(),
                        });
                }
            }
        }

        let dependents = Self::dependents(policies);
        let mut pending = levels
            .iter()
            .filter_map(|(package, bump)| {
                (*bump != BumpLevel::Unchanged).then_some(package.clone())
            })
            .collect::<BTreeSet<_>>();
        while let Some(dependency) = pending.pop_first() {
            let snapshot = graph
                .package(&dependency)
                .expect("release package ids are derived from the workspace graph");
            let dependency_next = bump_with_policy(
                &snapshot.version,
                levels[&dependency],
                &policies[&dependency],
            )
            .map_err(|source| ReleasePlannerError::InvalidVersionTransition {
                package: dependency.clone(),
                source,
            })?;

            for (dependent, requirement) in dependents.get(&dependency).into_iter().flatten() {
                if requirement
                    .as_ref()
                    .is_some_and(|requirement| requirement.matches(&dependency_next))
                {
                    continue;
                }
                reasons.entry(dependent.clone()).or_default().insert(
                    ReleaseReason::DependencyPropagation {
                        dependency: dependency.clone(),
                        next_version: dependency_next.clone(),
                    },
                );
                let level = levels
                    .get_mut(dependent)
                    .expect("validated policy dependents must have release levels");
                if *level == BumpLevel::Unchanged {
                    *level = BumpLevel::Patch;
                    pending.insert(dependent.clone());
                }
            }
        }

        let mut versions = VersionMap::new();
        let mut packages = Vec::new();
        let mut warnings = Vec::new();
        for snapshot in graph.packages() {
            let bump = levels[&snapshot.id];
            let next_version = bump_with_policy(&snapshot.version, bump, &policies[&snapshot.id])
                .map_err(|source| ReleasePlannerError::InvalidVersionTransition {
                package: snapshot.id.clone(),
                source,
            })?;
            versions.insert(snapshot.id.clone(), next_version.clone());
            if bump == BumpLevel::Unchanged {
                continue;
            }
            if matches!(policies[&snapshot.id].channel, ReleaseChannel::Stable)
                && !snapshot.version.pre.is_empty()
                && !matches!(bump, BumpLevel::Patch)
            {
                warnings.push(PlanWarning::NonPatchBumpOnPrerelease {
                    package: snapshot.id.clone(),
                    requested: bump,
                });
            }
            packages.push(PackageRelease {
                id: snapshot.id.clone(),
                ecosystem: snapshot.ecosystem,
                current_version: snapshot.version.clone(),
                next_version,
                bump,
                reasons: reasons
                    .remove(&snapshot.id)
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            });
        }

        let order = graph
            .topological_order()?
            .into_iter()
            .filter(|package| levels[package] != BumpLevel::Unchanged)
            .collect();
        ReleasePlan::new(
            packages,
            versions,
            order,
            changesets
                .iter()
                .map(|changeset| changeset.id.clone())
                .collect(),
            warnings,
            vec![],
        )
        .map_err(ReleasePlannerError::InvalidPlan)
    }

    fn validate_policies(
        graph: &WorkspaceGraph,
        policies: &ReleasePolicies,
    ) -> Result<(), ReleasePlannerError> {
        for package in graph.packages() {
            let policy = policies.get(&package.id).ok_or_else(|| {
                ReleasePlannerError::MissingPackagePolicy {
                    package: package.id.clone(),
                }
            })?;
            for dependency in policy.propagating_dependencies.keys() {
                if graph.package(dependency).is_none() {
                    return Err(ReleasePlannerError::UnknownPolicyDependency {
                        package: package.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
                if !package
                    .dependencies
                    .iter()
                    .any(|candidate| &candidate.package == dependency)
                {
                    return Err(ReleasePlannerError::PolicyDependencyIsNotGraphEdge {
                        package: package.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }
        if let Some(package) = policies
            .keys()
            .find(|package| graph.package(package).is_none())
        {
            return Err(ReleasePlannerError::UnknownPolicyPackage {
                package: package.clone(),
            });
        }
        Ok(())
    }

    fn dependents(
        policies: &ReleasePolicies,
    ) -> BTreeMap<PackageId, Vec<(PackageId, Option<VersionReq>)>> {
        let mut dependents = BTreeMap::<PackageId, Vec<_>>::new();
        for (dependent, policy) in policies {
            for (dependency, requirement) in &policy.propagating_dependencies {
                dependents
                    .entry(dependency.clone())
                    .or_default()
                    .push((dependent.clone(), requirement.clone()));
            }
        }
        dependents
    }
}

fn bump_with_policy(
    current: &semver::Version,
    changeset_bump: BumpLevel,
    policy: &PackageReleasePolicy,
) -> Result<semver::Version, VersioningError> {
    let bump = if current.pre.is_empty() && matches!(policy.channel, ReleaseChannel::Named(_)) {
        policy.channel_bump.unwrap_or(changeset_bump)
    } else {
        changeset_bump
    };
    if bump == BumpLevel::Unchanged && changeset_bump != BumpLevel::Unchanged {
        let mut next = current.clone();
        if let ReleaseChannel::Named(name) = &policy.channel {
            next.pre = semver::Prerelease::new(&format!("{name}.0")).map_err(|error| {
                VersioningError::InvalidChannel {
                    channel: name.clone(),
                    reason: error.to_string(),
                }
            })?;
        }
        return Ok(next);
    }
    bump_version(current, bump, &policy.channel)
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ReleasePlannerError {
    #[error("missing release policy for package: {package}")]
    MissingPackagePolicy { package: PackageId },
    #[error("release policy references unknown package: {package}")]
    UnknownPolicyPackage { package: PackageId },
    #[error("release policy for {package} references unknown dependency {dependency}")]
    UnknownPolicyDependency {
        package: PackageId,
        dependency: PackageId,
    },
    #[error("release policy for {package} references non-dependency {dependency}")]
    PolicyDependencyIsNotGraphEdge {
        package: PackageId,
        dependency: PackageId,
    },
    #[error("changeset {changeset} references unknown package {package}")]
    UnknownChangesetPackage {
        changeset: ChangesetId,
        package: PackageId,
    },
    #[error("invalid version transition for {package}: {source}")]
    InvalidVersionTransition {
        package: PackageId,
        source: VersioningError,
    },
    #[error(transparent)]
    Workspace(#[from] WorkspaceGraphError),
    #[error("invalid release plan: {0}")]
    InvalidPlan(ReleasePlanError),
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use semver::{Version, VersionReq};

    use super::*;
    use crate::{Dependency, DependencyKind, DependencySource, Ecosystem, PackageSnapshot};

    fn package(id: &str, version: &str, dependencies: &[&str]) -> PackageSnapshot {
        PackageSnapshot {
            id: PackageId::new(id),
            manifest_name: id.to_string(),
            version: Version::parse(version).unwrap(),
            ecosystem: Ecosystem::Rust,
            path: Utf8PathBuf::from(format!("crates/{id}")),
            publishable: true,
            dependencies: dependencies
                .iter()
                .map(|dependency| Dependency {
                    package: PackageId::new(*dependency),
                    kind: DependencyKind::Runtime,
                    requirement: None,
                    source: DependencySource::Manifest,
                })
                .collect(),
        }
    }

    fn policies(packages: &[(&str, ReleaseChannel)]) -> ReleasePolicies {
        packages
            .iter()
            .map(|(package, channel)| {
                (
                    PackageId::new(*package),
                    PackageReleasePolicy {
                        channel: channel.clone(),
                        channel_bump: None,
                        propagating_dependencies: BTreeMap::new(),
                    },
                )
            })
            .collect()
    }

    fn changeset(id: &str, releases: &[(&str, BumpLevel)]) -> ChangesetInput {
        ChangesetInput {
            id: ChangesetId::new(id),
            releases: releases
                .iter()
                .map(|(package, bump)| (PackageId::new(*package), *bump))
                .collect(),
        }
    }

    #[test]
    fn merges_changesets_and_keeps_a_complete_version_map() {
        let graph = WorkspaceGraph::new(vec![
            package("app", "1.0.0", &[]),
            package("unchanged", "2.0.0", &[]),
        ])
        .unwrap();
        let policies = policies(&[
            ("app", ReleaseChannel::Stable),
            ("unchanged", ReleaseChannel::Stable),
        ]);

        let plan = ReleasePlanner::plan(
            &graph,
            &[
                changeset("patch", &[("app", BumpLevel::Patch)]),
                changeset("feature", &[("app", BumpLevel::Minor)]),
            ],
            &policies,
        )
        .unwrap();

        let app = plan.package(&PackageId::new("app")).unwrap();
        assert_eq!(app.bump, BumpLevel::Minor);
        assert_eq!(app.next_version, Version::new(1, 1, 0));
        assert_eq!(app.reasons.len(), 2);
        assert_eq!(
            plan.versions()[&PackageId::new("unchanged")],
            Version::new(2, 0, 0)
        );
        assert_eq!(
            plan.consumed_changesets(),
            [ChangesetId::new("feature"), ChangesetId::new("patch")]
        );
    }

    #[test]
    fn channel_bump_overrides_only_the_first_named_channel_base() {
        let graph = WorkspaceGraph::new(vec![package("app", "0.1.0", &[])]).unwrap();
        for (override_bump, expected) in [
            (BumpLevel::Unchanged, "0.1.0-alpha.0"),
            (BumpLevel::Patch, "0.1.1-alpha.0"),
            (BumpLevel::Minor, "0.2.0-alpha.0"),
            (BumpLevel::Major, "1.0.0-alpha.0"),
        ] {
            let mut policies = policies(&[("app", ReleaseChannel::Named("alpha".into()))]);
            policies
                .get_mut(&PackageId::new("app"))
                .unwrap()
                .channel_bump = Some(override_bump);
            let plan = ReleasePlanner::plan(
                &graph,
                &[changeset("feature", &[("app", BumpLevel::Patch)])],
                &policies,
            )
            .unwrap();
            assert_eq!(
                plan.versions()[&PackageId::new("app")].to_string(),
                expected
            );
        }
    }

    #[test]
    fn propagates_transitively_only_when_constraints_stop_matching() {
        let graph = WorkspaceGraph::new(vec![
            package("app", "1.0.0", &["middle"]),
            package("middle", "1.0.0", &["core"]),
            package("core", "1.0.0", &[]),
        ])
        .unwrap();
        let mut policies = policies(&[
            ("app", ReleaseChannel::Stable),
            ("middle", ReleaseChannel::Stable),
            ("core", ReleaseChannel::Stable),
        ]);
        policies
            .get_mut(&PackageId::new("middle"))
            .unwrap()
            .propagating_dependencies
            .insert(
                PackageId::new("core"),
                Some(VersionReq::parse("~1.0.0").unwrap()),
            );
        policies
            .get_mut(&PackageId::new("app"))
            .unwrap()
            .propagating_dependencies
            .insert(
                PackageId::new("middle"),
                Some(VersionReq::parse("=1.0.0").unwrap()),
            );

        let plan = ReleasePlanner::plan(
            &graph,
            &[changeset("core-feature", &[("core", BumpLevel::Minor)])],
            &policies,
        )
        .unwrap();

        assert_eq!(plan.order(), ["core", "middle", "app"].map(PackageId::new));
        assert_eq!(
            plan.package(&PackageId::new("middle")).unwrap().bump,
            BumpLevel::Patch
        );
        assert_eq!(
            plan.package(&PackageId::new("app")).unwrap().bump,
            BumpLevel::Patch
        );
    }

    #[test]
    fn does_not_propagate_when_the_next_version_matches() {
        let graph = WorkspaceGraph::new(vec![
            package("app", "1.0.0", &["core"]),
            package("core", "1.0.0", &[]),
        ])
        .unwrap();
        let mut policies = policies(&[
            ("app", ReleaseChannel::Stable),
            ("core", ReleaseChannel::Stable),
        ]);
        policies
            .get_mut(&PackageId::new("app"))
            .unwrap()
            .propagating_dependencies
            .insert(
                PackageId::new("core"),
                Some(VersionReq::parse("^1").unwrap()),
            );

        let plan = ReleasePlanner::plan(
            &graph,
            &[changeset("core-feature", &[("core", BumpLevel::Minor)])],
            &policies,
        )
        .unwrap();

        assert!(plan.package(&PackageId::new("app")).is_none());
        assert_eq!(plan.order(), [PackageId::new("core")]);
    }

    #[test]
    fn keeps_explicit_bump_and_adds_dependency_reason() {
        let graph = WorkspaceGraph::new(vec![
            package("app", "1.0.0", &["core"]),
            package("core", "1.0.0", &[]),
        ])
        .unwrap();
        let mut policies = policies(&[
            ("app", ReleaseChannel::Stable),
            ("core", ReleaseChannel::Stable),
        ]);
        policies
            .get_mut(&PackageId::new("app"))
            .unwrap()
            .propagating_dependencies
            .insert(PackageId::new("core"), None);

        let plan = ReleasePlanner::plan(
            &graph,
            &[
                changeset("core-feature", &[("core", BumpLevel::Minor)]),
                changeset("app-breaking", &[("app", BumpLevel::Major)]),
            ],
            &policies,
        )
        .unwrap();

        let app = plan.package(&PackageId::new("app")).unwrap();
        assert_eq!(app.bump, BumpLevel::Major);
        assert!(app.reasons.iter().any(|reason| matches!(
            reason,
            ReleaseReason::DependencyPropagation { dependency, .. }
                if dependency == &PackageId::new("core")
        )));
    }

    #[test]
    fn emits_prerelease_warning_and_is_deterministic() {
        let graph = WorkspaceGraph::new(vec![package("app", "2.0.0-alpha.1", &[])]).unwrap();
        let policies = policies(&[("app", ReleaseChannel::Stable)]);
        let first = ReleasePlanner::plan(
            &graph,
            &[
                changeset("minor", &[("app", BumpLevel::Minor)]),
                changeset("major", &[("app", BumpLevel::Major)]),
            ],
            &policies,
        )
        .unwrap();
        let second = ReleasePlanner::plan(
            &graph,
            &[
                changeset("major", &[("app", BumpLevel::Major)]),
                changeset("minor", &[("app", BumpLevel::Minor)]),
            ],
            &policies,
        )
        .unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first.warnings(),
            [PlanWarning::NonPatchBumpOnPrerelease {
                package: PackageId::new("app"),
                requested: BumpLevel::Major,
            }]
        );
        assert_eq!(
            first.package(&PackageId::new("app")).unwrap().next_version,
            Version::new(2, 0, 0)
        );
    }

    #[test]
    fn rejects_missing_package_policy() {
        let graph = WorkspaceGraph::new(vec![package("app", "1.0.0", &[])]).unwrap();
        let error = ReleasePlanner::plan(&graph, &[], &BTreeMap::new()).unwrap_err();

        assert_eq!(
            error,
            ReleasePlannerError::MissingPackagePolicy {
                package: PackageId::new("app"),
            }
        );
    }

    #[test]
    fn rejects_changesets_for_unknown_packages() {
        let graph = WorkspaceGraph::new(vec![package("app", "1.0.0", &[])]).unwrap();
        let policies = policies(&[("app", ReleaseChannel::Stable)]);
        let error = ReleasePlanner::plan(
            &graph,
            &[changeset("unknown", &[("missing", BumpLevel::Patch)])],
            &policies,
        )
        .unwrap_err();

        assert_eq!(
            error,
            ReleasePlannerError::UnknownChangesetPackage {
                changeset: ChangesetId::new("unknown"),
                package: PackageId::new("missing"),
            }
        );
    }
}
