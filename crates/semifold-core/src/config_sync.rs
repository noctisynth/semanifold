use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8PathBuf;
use serde::Serialize;

use crate::{ChangesetId, Ecosystem, PackageId};

/// One package table read from the current Semifold configuration.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ConfiguredPackage {
    pub id: PackageId,
    pub ecosystem: Ecosystem,
    pub path: Utf8PathBuf,
}

/// One package returned by a complete ecosystem discovery pass.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DiscoveredPackage {
    pub id: PackageId,
    pub ecosystem: Ecosystem,
    pub path: Utf8PathBuf,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PackageRename {
    pub from: PackageId,
    pub to: PackageId,
    pub ecosystem: Ecosystem,
    pub path: Utf8PathBuf,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PackageMove {
    pub package: PackageId,
    pub ecosystem: Ecosystem,
    pub from: Utf8PathBuf,
    pub to: Utf8PathBuf,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConfigConflict {
    AmbiguousMatch {
        configured: Vec<ConfiguredPackage>,
        discovered: Vec<DiscoveredPackage>,
    },
    ResolverChanged {
        configured: ConfiguredPackage,
        discovered: DiscoveredPackage,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChangesetReference {
    pub changeset: ChangesetId,
    pub packages: BTreeSet<PackageId>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConfigSyncWarning {
    ChangesetReferencesRenamedPackage {
        changeset: ChangesetId,
        from: PackageId,
        to: PackageId,
    },
}

/// Deterministic, side-effect-free description of configuration drift.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConfigSyncPlan {
    pub config_path: Utf8PathBuf,
    pub added: Vec<DiscoveredPackage>,
    pub missing: Vec<ConfiguredPackage>,
    pub renamed: Vec<PackageRename>,
    pub moved: Vec<PackageMove>,
    pub conflicts: Vec<ConfigConflict>,
    pub warnings: Vec<ConfigSyncWarning>,
}

impl ConfigSyncPlan {
    #[must_use]
    pub fn has_drift(&self) -> bool {
        !self.added.is_empty()
            || !self.missing.is_empty()
            || !self.renamed.is_empty()
            || !self.moved.is_empty()
            || !self.conflicts.is_empty()
    }
}

/// Matches normalized configuration and discovery snapshots without I/O.
pub struct ConfigSyncPlanner;

impl ConfigSyncPlanner {
    #[must_use]
    pub fn plan(
        config_path: Utf8PathBuf,
        configured: &[ConfiguredPackage],
        discovered: &[DiscoveredPackage],
        changesets: &[ChangesetReference],
    ) -> ConfigSyncPlan {
        let mut configured = configured.to_vec();
        let mut discovered = discovered.to_vec();
        configured.sort();
        discovered.sort();

        let mut matches = MatchTracker::default();
        let mut renamed = Vec::new();
        let mut moved = Vec::new();

        let mut discovered_by_id = BTreeMap::<PackageId, Vec<_>>::new();
        for (index, package) in discovered.iter().enumerate() {
            discovered_by_id
                .entry(package.id.clone())
                .or_default()
                .push(index);
        }
        for (id, discovered_indexes) in discovered_by_id {
            if discovered_indexes.len() < 2 {
                continue;
            }
            let configured_indexes = configured
                .iter()
                .enumerate()
                .filter_map(|(index, package)| {
                    (package.id == id
                        || discovered_indexes.iter().any(|discovered_index| {
                            let found = &discovered[*discovered_index];
                            package.ecosystem == found.ecosystem && package.path == found.path
                        }))
                    .then_some(index)
                })
                .collect::<Vec<_>>();
            matches.push_ambiguous(
                &configured_indexes,
                &discovered_indexes,
                &configured,
                &discovered,
            );
        }

        let mut configured_by_location = BTreeMap::<(Ecosystem, Utf8PathBuf), Vec<_>>::new();
        let mut discovered_by_location = BTreeMap::<(Ecosystem, Utf8PathBuf), Vec<_>>::new();
        for (index, package) in configured.iter().enumerate() {
            if matches.configured.contains(&index) {
                continue;
            }
            configured_by_location
                .entry((package.ecosystem, package.path.clone()))
                .or_default()
                .push(index);
        }
        for (index, package) in discovered.iter().enumerate() {
            if matches.discovered.contains(&index) {
                continue;
            }
            discovered_by_location
                .entry((package.ecosystem, package.path.clone()))
                .or_default()
                .push(index);
        }
        for (location, configured_indexes) in configured_by_location {
            let Some(discovered_indexes) = discovered_by_location.get(&location) else {
                continue;
            };
            if configured_indexes.len() == 1 && discovered_indexes.len() == 1 {
                let configured_index = configured_indexes[0];
                let discovered_index = discovered_indexes[0];
                let current = &configured[configured_index];
                let found = &discovered[discovered_index];
                if current.id != found.id {
                    renamed.push(PackageRename {
                        from: current.id.clone(),
                        to: found.id.clone(),
                        ecosystem: found.ecosystem,
                        path: found.path.clone(),
                    });
                }
                matches.configured.insert(configured_index);
                matches.discovered.insert(discovered_index);
            } else {
                matches.push_ambiguous(
                    &configured_indexes,
                    discovered_indexes,
                    &configured,
                    &discovered,
                );
            }
        }

        let mut configured_by_id = BTreeMap::<PackageId, Vec<_>>::new();
        let mut discovered_by_id = BTreeMap::<PackageId, Vec<_>>::new();
        for (index, package) in configured.iter().enumerate() {
            if !matches.configured.contains(&index) {
                configured_by_id
                    .entry(package.id.clone())
                    .or_default()
                    .push(index);
            }
        }
        for (index, package) in discovered.iter().enumerate() {
            if !matches.discovered.contains(&index) {
                discovered_by_id
                    .entry(package.id.clone())
                    .or_default()
                    .push(index);
            }
        }
        for (id, configured_indexes) in configured_by_id {
            let Some(discovered_indexes) = discovered_by_id.get(&id) else {
                continue;
            };
            if configured_indexes.len() == 1 && discovered_indexes.len() == 1 {
                let configured_index = configured_indexes[0];
                let discovered_index = discovered_indexes[0];
                let current = &configured[configured_index];
                let found = &discovered[discovered_index];
                if current.ecosystem == found.ecosystem {
                    moved.push(PackageMove {
                        package: found.id.clone(),
                        ecosystem: found.ecosystem,
                        from: current.path.clone(),
                        to: found.path.clone(),
                    });
                } else {
                    matches.conflicts.push(ConfigConflict::ResolverChanged {
                        configured: current.clone(),
                        discovered: found.clone(),
                    });
                }
                matches.configured.insert(configured_index);
                matches.discovered.insert(discovered_index);
            } else {
                matches.push_ambiguous(
                    &configured_indexes,
                    discovered_indexes,
                    &configured,
                    &discovered,
                );
            }
        }

        let mut configured_by_path = BTreeMap::<Utf8PathBuf, Vec<_>>::new();
        let mut discovered_by_path = BTreeMap::<Utf8PathBuf, Vec<_>>::new();
        for (index, package) in configured.iter().enumerate() {
            if !matches.configured.contains(&index) {
                configured_by_path
                    .entry(package.path.clone())
                    .or_default()
                    .push(index);
            }
        }
        for (index, package) in discovered.iter().enumerate() {
            if !matches.discovered.contains(&index) {
                discovered_by_path
                    .entry(package.path.clone())
                    .or_default()
                    .push(index);
            }
        }
        for (path, configured_indexes) in configured_by_path {
            let Some(discovered_indexes) = discovered_by_path.get(&path) else {
                continue;
            };
            if configured_indexes.len() == 1 && discovered_indexes.len() == 1 {
                let configured_index = configured_indexes[0];
                let discovered_index = discovered_indexes[0];
                matches.conflicts.push(ConfigConflict::ResolverChanged {
                    configured: configured[configured_index].clone(),
                    discovered: discovered[discovered_index].clone(),
                });
                matches.configured.insert(configured_index);
                matches.discovered.insert(discovered_index);
            } else {
                matches.push_ambiguous(
                    &configured_indexes,
                    discovered_indexes,
                    &configured,
                    &discovered,
                );
            }
        }

        let added = discovered
            .into_iter()
            .enumerate()
            .filter_map(|(index, package)| {
                (!matches.discovered.contains(&index)).then_some(package)
            })
            .collect();
        let missing = configured
            .into_iter()
            .enumerate()
            .filter_map(|(index, package)| {
                (!matches.configured.contains(&index)).then_some(package)
            })
            .collect();
        renamed.sort();
        moved.sort();
        matches.conflicts.sort();
        let warnings = renamed
            .iter()
            .flat_map(|rename| {
                changesets
                    .iter()
                    .filter(|changeset| changeset.packages.contains(&rename.from))
                    .map(
                        |changeset| ConfigSyncWarning::ChangesetReferencesRenamedPackage {
                            changeset: changeset.changeset.clone(),
                            from: rename.from.clone(),
                            to: rename.to.clone(),
                        },
                    )
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        ConfigSyncPlan {
            config_path,
            added,
            missing,
            renamed,
            moved,
            conflicts: matches.conflicts,
            warnings,
        }
    }
}

#[derive(Default)]
struct MatchTracker {
    configured: BTreeSet<usize>,
    discovered: BTreeSet<usize>,
    conflicts: Vec<ConfigConflict>,
}

impl MatchTracker {
    fn push_ambiguous(
        &mut self,
        configured_indexes: &[usize],
        discovered_indexes: &[usize],
        configured: &[ConfiguredPackage],
        discovered: &[DiscoveredPackage],
    ) {
        self.conflicts.push(ConfigConflict::AmbiguousMatch {
            configured: configured_indexes
                .iter()
                .map(|index| configured[*index].clone())
                .collect(),
            discovered: discovered_indexes
                .iter()
                .map(|index| discovered[*index].clone())
                .collect(),
        });
        self.configured.extend(configured_indexes);
        self.discovered.extend(discovered_indexes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured(id: &str, ecosystem: Ecosystem, path: &str) -> ConfiguredPackage {
        ConfiguredPackage {
            id: PackageId::new(id),
            ecosystem,
            path: Utf8PathBuf::from(path),
        }
    }

    fn discovered(id: &str, ecosystem: Ecosystem, path: &str) -> DiscoveredPackage {
        DiscoveredPackage {
            id: PackageId::new(id),
            ecosystem,
            path: Utf8PathBuf::from(path),
        }
    }

    fn plan(configured: &[ConfiguredPackage], discovered: &[DiscoveredPackage]) -> ConfigSyncPlan {
        ConfigSyncPlanner::plan(
            Utf8PathBuf::from(".changes/config.toml"),
            configured,
            discovered,
            &[],
        )
    }

    #[test]
    fn classifies_added_missing_renamed_and_moved_packages() {
        let plan = plan(
            &[
                configured("missing", Ecosystem::Rust, "crates/missing"),
                configured("moved", Ecosystem::Node, "packages/old"),
                configured("old-name", Ecosystem::Python, "python/pkg"),
                configured("same", Ecosystem::Cpp, "cpp/same"),
            ],
            &[
                discovered("added", Ecosystem::Rust, "crates/added"),
                discovered("moved", Ecosystem::Node, "packages/new"),
                discovered("new-name", Ecosystem::Python, "python/pkg"),
                discovered("same", Ecosystem::Cpp, "cpp/same"),
            ],
        );

        assert_eq!(
            plan.added,
            [discovered("added", Ecosystem::Rust, "crates/added")]
        );
        assert_eq!(
            plan.missing,
            [configured("missing", Ecosystem::Rust, "crates/missing")]
        );
        assert_eq!(
            plan.renamed,
            [PackageRename {
                from: PackageId::new("old-name"),
                to: PackageId::new("new-name"),
                ecosystem: Ecosystem::Python,
                path: Utf8PathBuf::from("python/pkg"),
            }]
        );
        assert_eq!(
            plan.moved,
            [PackageMove {
                package: PackageId::new("moved"),
                ecosystem: Ecosystem::Node,
                from: Utf8PathBuf::from("packages/old"),
                to: Utf8PathBuf::from("packages/new"),
            }]
        );
        assert!(plan.conflicts.is_empty());
        assert!(plan.has_drift());
    }

    #[test]
    fn reports_resolver_changes_without_adding_or_removing_the_package() {
        let current = configured("app", Ecosystem::Rust, "app");
        let found = discovered("app", Ecosystem::Node, "app");

        let plan = plan(std::slice::from_ref(&current), std::slice::from_ref(&found));

        assert_eq!(
            plan.conflicts,
            [ConfigConflict::ResolverChanged {
                configured: current,
                discovered: found,
            }]
        );
        assert!(plan.added.is_empty());
        assert!(plan.missing.is_empty());
    }

    #[test]
    fn reports_ambiguous_path_matches_only_as_conflicts() {
        let plan = plan(
            &[
                configured("first", Ecosystem::Rust, "crates/shared"),
                configured("second", Ecosystem::Rust, "crates/shared"),
            ],
            &[discovered("found", Ecosystem::Rust, "crates/shared")],
        );

        assert_eq!(
            plan.conflicts,
            [ConfigConflict::AmbiguousMatch {
                configured: vec![
                    configured("first", Ecosystem::Rust, "crates/shared"),
                    configured("second", Ecosystem::Rust, "crates/shared"),
                ],
                discovered: vec![discovered("found", Ecosystem::Rust, "crates/shared")],
            }]
        );
        assert!(plan.added.is_empty());
        assert!(plan.missing.is_empty());
    }

    #[test]
    fn rejects_duplicate_discovered_package_ids() {
        let first = discovered("shared", Ecosystem::Rust, "crates/shared");
        let second = discovered("shared", Ecosystem::Node, "packages/shared");

        let plan = plan(&[], &[second.clone(), first.clone()]);

        assert_eq!(
            plan.conflicts,
            [ConfigConflict::AmbiguousMatch {
                configured: vec![],
                discovered: vec![first, second],
            }]
        );
        assert!(plan.added.is_empty());
    }

    #[test]
    fn produces_the_same_plan_for_any_input_order() {
        let mut configured = vec![
            configured("missing", Ecosystem::Rust, "crates/missing"),
            configured("moved", Ecosystem::Rust, "crates/old"),
        ];
        let mut discovered = vec![
            discovered("added", Ecosystem::Rust, "crates/added"),
            discovered("moved", Ecosystem::Rust, "crates/new"),
        ];
        let first = plan(&configured, &discovered);
        configured.reverse();
        discovered.reverse();

        assert_eq!(first, plan(&configured, &discovered));
    }

    #[test]
    fn reports_no_drift_for_identical_snapshots() {
        let current = configured("app", Ecosystem::Rust, "crates/app");
        let found = discovered("app", Ecosystem::Rust, "crates/app");

        assert!(!plan(&[current], &[found]).has_drift());
    }

    #[test]
    fn warns_when_pending_changesets_reference_a_renamed_package() {
        let changesets = [ChangesetReference {
            changeset: ChangesetId::new("pending"),
            packages: BTreeSet::from([PackageId::new("old-name")]),
        }];

        let plan = ConfigSyncPlanner::plan(
            Utf8PathBuf::from(".changes/config.toml"),
            &[configured("old-name", Ecosystem::Rust, "crates/app")],
            &[discovered("new-name", Ecosystem::Rust, "crates/app")],
            &changesets,
        );

        assert_eq!(
            plan.warnings,
            [ConfigSyncWarning::ChangesetReferencesRenamedPackage {
                changeset: ChangesetId::new("pending"),
                from: PackageId::new("old-name"),
                to: PackageId::new("new-name"),
            }]
        );
    }
}
