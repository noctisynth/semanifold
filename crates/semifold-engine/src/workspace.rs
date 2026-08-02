use std::{collections::BTreeMap, path::Path};

use anyhow::Context as _;
use semifold_core::{
    Dependency, DependencyKind, DependencySource, PackageId, PackageSnapshot, WorkspaceGraph,
};
use semifold_resolver::{
    adapter::{PackageInspection, PackageLocation},
    config::Config,
};

use crate::{discovery::ResolverRegistry, package_path::normalize_package_path};

#[derive(Debug)]
struct ConfiguredInspection {
    package: PackageInspection,
    explicit_dependencies: Vec<PackageId>,
}

pub fn load_workspace_graph(root: &Path, config: &Config) -> anyhow::Result<WorkspaceGraph> {
    let mut resolved = Vec::with_capacity(config.packages.len());
    let registry = ResolverRegistry;
    for (id, package_config) in &config.packages {
        let project_root =
            camino::Utf8PathBuf::from_path_buf(root.to_path_buf()).map_err(|path| {
                anyhow::anyhow!("project root is not valid UTF-8: {}", path.display())
            })?;
        let path = normalize_package_path(root, &package_config.path)?;
        let inspection =
            registry
                .create_adapter(package_config.resolver)
                .inspect(&PackageLocation {
                    id: PackageId::new(id),
                    project_root,
                    path,
                })?;
        resolved.push(ConfiguredInspection {
            package: inspection,
            explicit_dependencies: package_config.depends_on.clone(),
        });
    }
    workspace_graph_from_inspections(root, resolved)
}

fn workspace_graph_from_inspections(
    root: &Path,
    resolved: Vec<ConfiguredInspection>,
) -> anyhow::Result<WorkspaceGraph> {
    let mut package_ids = BTreeMap::new();
    for package in &resolved {
        let key = (
            package.package.ecosystem,
            package.package.manifest_name.clone(),
        );
        anyhow::ensure!(
            package_ids
                .insert(key, package.package.id.clone())
                .is_none(),
            "duplicate manifest package name {} in {:?}",
            package.package.manifest_name,
            package.package.ecosystem
        );
    }

    let snapshots = resolved
        .into_iter()
        .map(|resolved| {
            let path = normalize_package_path(root, resolved.package.path.as_std_path())?;
            let mut dependencies = resolved
                .package
                .dependencies
                .into_iter()
                .filter_map(|dependency| {
                    package_ids
                        .get(&(resolved.package.ecosystem, dependency.manifest_name))
                        .cloned()
                        .map(|package| Dependency {
                            package,
                            kind: dependency.kind,
                            requirement: dependency.requirement,
                            source: DependencySource::Manifest,
                        })
                })
                .collect::<Vec<_>>();
            dependencies.extend(resolved.explicit_dependencies.into_iter().map(|package| {
                Dependency {
                    package,
                    kind: DependencyKind::Unspecified,
                    requirement: None,
                    source: DependencySource::Config,
                }
            }));
            Ok(PackageSnapshot {
                id: resolved.package.id,
                manifest_name: resolved.package.manifest_name,
                version: resolved.package.version,
                version_source: resolved.package.version_source,
                ecosystem: resolved.package.ecosystem,
                path,
                publishable: resolved.package.publishable,
                dependencies,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    WorkspaceGraph::new(snapshots).context("failed to build workspace graph")
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use semifold_core::{DependencyKind, DependencySource, PackageId, WorkspaceGraphError};
    use semifold_resolver::{
        config::{BranchesConfig, PackageConfig, ReleaseChannel},
        resolver::ResolverType,
    };

    use super::*;

    struct TemporaryRoot(PathBuf);

    static NEXT_TEMPORARY_ROOT: AtomicU64 = AtomicU64::new(0);

    impl TemporaryRoot {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "semifold-workspace-snapshot-{}-{nonce}-{}",
                std::process::id(),
                NEXT_TEMPORARY_ROOT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, path: &str, content: &str) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }

    impl Drop for TemporaryRoot {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn package(path: &str, resolver: ResolverType) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver,
            channel: ReleaseChannel::Stable,
            channel_bump: None,
            assets: vec![],
            depends_on: vec![],
        }
    }

    #[test]
    fn builds_a_real_mixed_ecosystem_graph_without_cross_ecosystem_name_collisions() {
        let root = TemporaryRoot::new();
        root.write(
            "rust/core/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"1.0.0\"\n",
        );
        root.write(
            "rust/api/Cargo.toml",
            "[package]\nname = \"rust-api\"\nversion = \"1.0.0\"\n\n[dependencies]\nshared = { version = \"1\", path = \"../core\" }\nserde = \"1\"\n",
        );
        root.write(
            "node/core/package.json",
            r#"{"name":"shared","version":"1.0.0"}"#,
        );
        root.write(
            "node/app/package.json",
            r#"{"name":"node-app","version":"1.0.0","dependencies":{"shared":"workspace:*","lodash":"^4"}}"#,
        );
        root.write(
            "python/core/pyproject.toml",
            "[project]\nname = \"shared\"\nversion = \"1.0.0\"\n",
        );
        root.write(
            "python/app/pyproject.toml",
            "[project]\nname = \"python-app\"\nversion = \"1.0.0\"\ndependencies = [\"shared>=1\", \"requests>=2\"]\n",
        );
        root.write("cpp/core/CMakeLists.txt", "project(shared VERSION 1.0.0)\n");
        root.write(
            "cpp/app/CMakeLists.txt",
            "project(cpp_app VERSION 1.0.0)\ntarget_link_libraries(cpp_app PRIVATE shared external)\n",
        );

        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("cpp-app".to_string(), package("cpp/app", ResolverType::Cpp)),
                (
                    "cpp-core".to_string(),
                    package("cpp/core", ResolverType::Cpp),
                ),
                (
                    "node-app".to_string(),
                    package("node/app", ResolverType::Nodejs),
                ),
                (
                    "node-core".to_string(),
                    package("node/core", ResolverType::Nodejs),
                ),
                (
                    "python-app".to_string(),
                    package("python/app", ResolverType::Python),
                ),
                (
                    "python-core".to_string(),
                    package("python/core", ResolverType::Python),
                ),
                (
                    "rust-api".to_string(),
                    package("rust/api", ResolverType::Rust),
                ),
                (
                    "rust-core".to_string(),
                    package("rust/core", ResolverType::Rust),
                ),
            ]),
            resolver: BTreeMap::new(),
        };

        let graph = load_workspace_graph(&root.0, &config).unwrap();

        assert_eq!(
            graph.topological_order().unwrap(),
            [
                "cpp-core",
                "cpp-app",
                "node-core",
                "node-app",
                "python-core",
                "python-app",
                "rust-core",
                "rust-api",
            ]
            .map(PackageId::new)
            .to_vec()
        );
        let rust_api = graph.package(&PackageId::new("rust-api")).unwrap();
        assert_eq!(rust_api.manifest_name, "rust-api");
        assert_eq!(rust_api.dependencies.len(), 1);
        assert_eq!(
            rust_api.dependencies[0].package,
            PackageId::new("rust-core")
        );
        assert_eq!(rust_api.dependencies[0].kind, DependencyKind::Runtime);
        assert_eq!(rust_api.dependencies[0].requirement.as_deref(), Some("1"));
        assert_eq!(
            graph
                .package(&PackageId::new("node-app"))
                .unwrap()
                .dependencies[0]
                .package,
            PackageId::new("node-core")
        );
        let python_app = graph.package(&PackageId::new("python-app")).unwrap();
        assert_eq!(python_app.dependencies.len(), 1);
        assert_eq!(
            python_app.dependencies[0].package,
            PackageId::new("python-core")
        );
        assert_eq!(
            python_app.dependencies[0].requirement.as_deref(),
            Some(">=1")
        );
        assert_eq!(
            graph
                .package(&PackageId::new("cpp-app"))
                .unwrap()
                .dependencies[0]
                .package,
            PackageId::new("cpp-core")
        );
    }

    #[test]
    fn merges_explicit_cross_ecosystem_dependencies_into_the_workspace_graph() {
        let root = TemporaryRoot::new();
        root.write(
            "rust/core/Cargo.toml",
            "[package]\nname = \"native-core\"\nversion = \"1.0.0\"\n",
        );
        root.write(
            "node/binding/package.json",
            r#"{"name":"node-binding","version":"1.0.0"}"#,
        );
        let mut binding = package("node/binding", ResolverType::Nodejs);
        binding.depends_on = vec![PackageId::new("rust-core")];
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("node-binding".to_string(), binding),
                (
                    "rust-core".to_string(),
                    package("rust/core", ResolverType::Rust),
                ),
            ]),
            resolver: BTreeMap::new(),
        };

        let graph = load_workspace_graph(&root.0, &config).unwrap();

        assert_eq!(
            graph.topological_order().unwrap(),
            [PackageId::new("rust-core"), PackageId::new("node-binding")]
        );
        let dependency = &graph
            .package(&PackageId::new("node-binding"))
            .unwrap()
            .dependencies[0];
        assert_eq!(dependency.package, PackageId::new("rust-core"));
        assert_eq!(dependency.kind, DependencyKind::Unspecified);
        assert_eq!(dependency.source, DependencySource::Config);
        assert_eq!(dependency.requirement, None);
    }

    #[test]
    fn rejects_unknown_explicit_dependency_targets() {
        let root = TemporaryRoot::new();
        root.write(
            "node/binding/package.json",
            r#"{"name":"node-binding","version":"1.0.0"}"#,
        );
        let mut binding = package("node/binding", ResolverType::Nodejs);
        binding.depends_on = vec![PackageId::new("missing-core")];
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([("node-binding".to_string(), binding)]),
            resolver: BTreeMap::new(),
        };

        let error = load_workspace_graph(&root.0, &config).unwrap_err();

        assert_eq!(
            error.downcast_ref::<WorkspaceGraphError>(),
            Some(&WorkspaceGraphError::UnknownDependency {
                package: PackageId::new("node-binding"),
                dependency: PackageId::new("missing-core"),
            })
        );
    }

    #[test]
    fn rejects_cycles_formed_by_explicit_cross_ecosystem_dependencies() {
        let root = TemporaryRoot::new();
        root.write(
            "rust/core/Cargo.toml",
            "[package]\nname = \"native-core\"\nversion = \"1.0.0\"\n",
        );
        root.write(
            "node/binding/package.json",
            r#"{"name":"node-binding","version":"1.0.0"}"#,
        );
        let mut rust = package("rust/core", ResolverType::Rust);
        rust.depends_on = vec![PackageId::new("node-binding")];
        let mut node = package("node/binding", ResolverType::Nodejs);
        node.depends_on = vec![PackageId::new("rust-core")];
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("node-binding".to_string(), node),
                ("rust-core".to_string(), rust),
            ]),
            resolver: BTreeMap::new(),
        };

        let graph = load_workspace_graph(&root.0, &config).unwrap();
        let error = graph.topological_order().unwrap_err();

        assert_eq!(
            error,
            WorkspaceGraphError::DependencyCycle {
                cycle: [
                    PackageId::new("node-binding"),
                    PackageId::new("rust-core"),
                    PackageId::new("node-binding"),
                ]
                .to_vec(),
            }
        );
    }

    #[test]
    fn rejects_duplicate_manifest_names_within_one_ecosystem() {
        let root = TemporaryRoot::new();
        for path in ["first", "second"] {
            root.write(
                &format!("{path}/Cargo.toml"),
                "[package]\nname = \"duplicate\"\nversion = \"1.0.0\"\n",
            );
        }
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("first".to_string(), package("first", ResolverType::Rust)),
                ("second".to_string(), package("second", ResolverType::Rust)),
            ]),
            resolver: BTreeMap::new(),
        };

        let error = load_workspace_graph(&root.0, &config).unwrap_err();

        assert_eq!(
            error.to_string(),
            "duplicate manifest package name duplicate in Rust"
        );
    }
}
