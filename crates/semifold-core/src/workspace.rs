use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::{PackageId, PackageSnapshot};

/// Validated package graph for one multi-ecosystem workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceGraph {
    packages: BTreeMap<PackageId, PackageSnapshot>,
    dependencies: BTreeMap<PackageId, BTreeSet<PackageId>>,
}

impl WorkspaceGraph {
    /// Builds a graph from discovered packages and their internal dependencies.
    pub fn new(packages: Vec<PackageSnapshot>) -> Result<Self, WorkspaceGraphError> {
        let mut package_map = BTreeMap::new();

        for package in packages {
            let id = package.id.clone();
            if package_map.insert(id.clone(), package).is_some() {
                return Err(WorkspaceGraphError::DuplicatePackageId { package: id });
            }
        }

        let mut dependencies = BTreeMap::new();
        for (id, package) in &package_map {
            let mut package_dependencies = BTreeSet::new();
            for dependency in &package.dependencies {
                if !package_map.contains_key(&dependency.package) {
                    return Err(WorkspaceGraphError::UnknownDependency {
                        package: id.clone(),
                        dependency: dependency.package.clone(),
                    });
                }
                package_dependencies.insert(dependency.package.clone());
            }
            dependencies.insert(id.clone(), package_dependencies);
        }

        Ok(Self {
            packages: package_map,
            dependencies,
        })
    }

    #[must_use]
    pub fn package(&self, id: &PackageId) -> Option<&PackageSnapshot> {
        self.packages.get(id)
    }

    pub fn packages(&self) -> impl Iterator<Item = &PackageSnapshot> {
        self.packages.values()
    }

    /// Returns a stable order where every dependency precedes its dependents.
    pub fn topological_order(&self) -> Result<Vec<PackageId>, WorkspaceGraphError> {
        let mut remaining_dependencies = self.dependencies.clone();
        let mut order = Vec::with_capacity(self.packages.len());

        while let Some(id) = remaining_dependencies
            .iter()
            .find_map(|(id, dependencies)| dependencies.is_empty().then(|| id.clone()))
        {
            remaining_dependencies.remove(&id);
            for dependencies in remaining_dependencies.values_mut() {
                dependencies.remove(&id);
            }
            order.push(id);
        }

        if remaining_dependencies.is_empty() {
            Ok(order)
        } else {
            Err(WorkspaceGraphError::DependencyCycle {
                cycle: self.find_cycle(&remaining_dependencies),
            })
        }
    }

    fn find_cycle(
        &self,
        remaining_dependencies: &BTreeMap<PackageId, BTreeSet<PackageId>>,
    ) -> Vec<PackageId> {
        let mut visited = BTreeSet::new();
        let mut stack = Vec::new();
        let mut visiting = BTreeSet::new();

        for id in remaining_dependencies.keys() {
            if let Some(cycle) = Self::visit(
                id,
                remaining_dependencies,
                &mut visited,
                &mut visiting,
                &mut stack,
            ) {
                return cycle;
            }
        }

        unreachable!("a non-empty graph with no topological order contains a cycle")
    }

    fn visit(
        id: &PackageId,
        remaining_dependencies: &BTreeMap<PackageId, BTreeSet<PackageId>>,
        visited: &mut BTreeSet<PackageId>,
        visiting: &mut BTreeSet<PackageId>,
        stack: &mut Vec<PackageId>,
    ) -> Option<Vec<PackageId>> {
        if let Some(cycle_start) = stack.iter().position(|package| package == id) {
            let mut cycle = stack[cycle_start..].to_vec();
            cycle.push(id.clone());
            return Some(cycle);
        }
        if !visited.insert(id.clone()) {
            return None;
        }

        visiting.insert(id.clone());
        stack.push(id.clone());
        for dependency in &remaining_dependencies[id] {
            if let Some(cycle) =
                Self::visit(dependency, remaining_dependencies, visited, visiting, stack)
            {
                return Some(cycle);
            }
        }
        stack.pop();
        visiting.remove(id);
        None
    }
}

/// Validation and ordering failures produced by [`WorkspaceGraph`].
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum WorkspaceGraphError {
    #[error("duplicate package id: {package}")]
    DuplicatePackageId { package: PackageId },
    #[error("package {package} depends on unknown package {dependency}")]
    UnknownDependency {
        package: PackageId,
        dependency: PackageId,
    },
    #[error("dependency cycle: {}", display_cycle(.cycle))]
    DependencyCycle { cycle: Vec<PackageId> },
}

fn display_cycle(cycle: &[PackageId]) -> DependencyCycleDisplay<'_> {
    DependencyCycleDisplay(cycle)
}

struct DependencyCycleDisplay<'a>(&'a [PackageId]);

impl fmt::Display for DependencyCycleDisplay<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, package) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str(" -> ")?;
            }
            package.fmt(formatter)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use semver::Version;

    use super::*;
    use crate::{Dependency, DependencyKind, DependencySource, Ecosystem};

    fn package(id: &str, dependencies: &[&str]) -> PackageSnapshot {
        PackageSnapshot {
            id: PackageId::new(id),
            manifest_name: id.to_owned(),
            version: Version::new(1, 0, 0),
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

    fn ids(order: Vec<PackageId>) -> Vec<String> {
        order.into_iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn orders_multi_level_dependencies() {
        let graph = WorkspaceGraph::new(vec![
            package("app", &["api"]),
            package("api", &["core"]),
            package("core", &[]),
        ])
        .unwrap();

        assert_eq!(
            ids(graph.topological_order().unwrap()),
            ["core", "api", "app"]
        );
    }

    #[test]
    fn orders_diamond_dependencies_once() {
        let graph = WorkspaceGraph::new(vec![
            package("app", &["left", "right"]),
            package("left", &["core"]),
            package("right", &["core"]),
            package("core", &[]),
        ])
        .unwrap();

        assert_eq!(
            ids(graph.topological_order().unwrap()),
            ["core", "left", "right", "app"]
        );
    }

    #[test]
    fn orders_every_manifest_dependency_kind_before_the_dependent() {
        let mut app = package(
            "app",
            &["runtime", "development", "build", "optional", "peer"],
        );
        for (dependency, kind) in app.dependencies.iter_mut().zip([
            DependencyKind::Runtime,
            DependencyKind::Development,
            DependencyKind::Build,
            DependencyKind::Optional,
            DependencyKind::Peer,
        ]) {
            dependency.kind = kind;
        }
        let graph = WorkspaceGraph::new(vec![
            app,
            package("runtime", &[]),
            package("development", &[]),
            package("build", &[]),
            package("optional", &[]),
            package("peer", &[]),
        ])
        .unwrap();

        assert_eq!(
            ids(graph.topological_order().unwrap()),
            ["build", "development", "optional", "peer", "runtime", "app"]
        );
    }

    #[test]
    fn orders_unrelated_packages_by_package_id() {
        let graph =
            WorkspaceGraph::new(vec![package("zebra", &[]), package("alpha", &[])]).unwrap();

        assert_eq!(ids(graph.topological_order().unwrap()), ["alpha", "zebra"]);
    }

    #[test]
    fn reports_complete_dependency_cycle() {
        let graph = WorkspaceGraph::new(vec![
            package("a", &["b"]),
            package("b", &["c"]),
            package("c", &["a"]),
        ])
        .unwrap();

        assert_eq!(
            graph.topological_order(),
            Err(WorkspaceGraphError::DependencyCycle {
                cycle: vec![
                    PackageId::new("a"),
                    PackageId::new("b"),
                    PackageId::new("c"),
                    PackageId::new("a"),
                ],
            })
        );
        assert_eq!(
            graph.topological_order().unwrap_err().to_string(),
            "dependency cycle: a -> b -> c -> a"
        );
    }

    #[test]
    fn rejects_duplicate_package_ids() {
        assert_eq!(
            WorkspaceGraph::new(vec![package("core", &[]), package("core", &[])]),
            Err(WorkspaceGraphError::DuplicatePackageId {
                package: PackageId::new("core"),
            })
        );
    }

    #[test]
    fn rejects_unknown_internal_dependencies() {
        assert_eq!(
            WorkspaceGraph::new(vec![package("app", &["missing"])]),
            Err(WorkspaceGraphError::UnknownDependency {
                package: PackageId::new("app"),
                dependency: PackageId::new("missing"),
            })
        );
    }
}
