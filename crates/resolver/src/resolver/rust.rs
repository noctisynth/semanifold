use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use semifold_core::{
    DependencyKind, Ecosystem, EditSource, FileEdit, FileEditExpectation, FileHash, PackageId,
    PackageSnapshot, VersionMap,
};
use serde::Deserialize;

use crate::{
    adapter::{
        AdapterError, EcosystemAdapter, EcosystemPlanInput, ManifestDependency, PackageInspection,
        PackageLocation, ParsedPackage,
    },
    config::{PackageConfig, ReleaseChannel},
    error::ResolveError,
    resolver::ResolverType,
};

#[derive(Deserialize)]
struct CargoPackage {
    pub name: String,
    pub version: String,
    pub publish: Option<bool>,
}

#[derive(Deserialize)]
struct CargoWorkspace {
    #[serde(default)]
    pub members: Vec<String>,
    pub dependencies: Option<BTreeMap<String, serde_json::Value>>,
}

#[derive(Deserialize)]
struct CargoToml {
    pub package: Option<CargoPackage>,
    pub workspace: Option<CargoWorkspace>,
    pub dependencies: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(rename = "dev-dependencies")]
    pub dev_dependencies: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(rename = "build-dependencies")]
    pub build_dependencies: Option<BTreeMap<String, serde_json::Value>>,
}

pub struct RustResolver;

struct PlannedManifest {
    original: String,
    document: toml_edit::DocumentMut,
    package: Option<PackageId>,
    dependencies: BTreeSet<PackageId>,
}

impl RustResolver {
    fn package_config(path: impl Into<std::path::PathBuf>) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Rust,
            channel: ReleaseChannel::Stable,
            assets: Vec::new(),
            depends_on: vec![],
        }
    }

    fn package_inspection(
        id: PackageId,
        package: ParsedPackage,
        dependencies: Vec<ManifestDependency>,
    ) -> Result<PackageInspection, AdapterError> {
        let path = camino::Utf8PathBuf::from_path_buf(package.path).map_err(|path| {
            AdapterError::InvalidInput {
                reason: format!("Rust package path is not valid UTF-8: {}", path.display()),
            }
        })?;
        Ok(PackageInspection {
            id,
            manifest_name: package.name,
            version: package.version,
            ecosystem: Ecosystem::Rust,
            path,
            publishable: !package.private,
            dependencies,
        })
    }

    /// Plans package and shared workspace manifest replacements as one deterministic batch.
    pub fn plan_file_edits(
        root: &Path,
        released_packages: &[&PackageSnapshot],
        workspace_packages: &[&PackageSnapshot],
        versions: &VersionMap,
    ) -> Result<Vec<FileEdit>, ResolveError> {
        let changed_versions = workspace_packages
            .iter()
            .filter_map(|package| {
                versions
                    .get(&package.id)
                    .filter(|version| *version != &package.version)
                    .map(|version| (package, version))
            })
            .map(|(package, version)| {
                RustResolver
                    .encode_version(version)
                    .map(|version| (package.manifest_name.clone(), (package.id.clone(), version)))
                    .map_err(|error| ResolveError::InvalidVersion {
                        version: version.to_string(),
                        reason: error.to_string(),
                    })
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let mut manifests = BTreeMap::<String, PlannedManifest>::new();

        for package in released_packages {
            let relative_path = Self::manifest_path(package);
            let manifest = Self::load_manifest(root, &relative_path, &mut manifests)?;
            let next_version =
                versions
                    .get(&package.id)
                    .ok_or_else(|| ResolveError::InvalidConfig {
                        path: root.join(&relative_path),
                        reason: format!("missing planned version for {}", package.id),
                    })?;
            let package_table =
                manifest.document["package"]
                    .as_table_mut()
                    .ok_or(ResolveError::ParseError {
                        path: root.join(&relative_path),
                        reason: "package table not found".to_string(),
                    })?;
            package_table["version"] =
                toml_edit::value(RustResolver.encode_version(next_version).map_err(|error| {
                    ResolveError::InvalidVersion {
                        version: next_version.to_string(),
                        reason: error.to_string(),
                    }
                })?);
            manifest.package = Some(package.id.clone());

            for dependency_table in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(dependencies) = manifest.document[dependency_table].as_table_mut() {
                    manifest
                        .dependencies
                        .extend(Self::update_dependency_versions(
                            dependencies,
                            &changed_versions,
                        ));
                }
            }
        }

        let workspace_path = "Cargo.toml";
        let workspace_absolute = root.join(workspace_path);
        if workspace_absolute.exists() {
            let manifest = Self::load_manifest(root, workspace_path, &mut manifests)?;
            if let Some(dependencies) = manifest
                .document
                .get_mut("workspace")
                .and_then(toml_edit::Item::as_table_mut)
                .and_then(|workspace| workspace.get_mut("dependencies"))
                .and_then(toml_edit::Item::as_table_mut)
            {
                manifest
                    .dependencies
                    .extend(Self::update_dependency_versions(
                        dependencies,
                        &changed_versions,
                    ));
            }
        }

        manifests
            .into_iter()
            .filter_map(|(path, manifest)| {
                let new_content = manifest.document.to_string();
                (new_content != manifest.original).then_some((path, manifest, new_content))
            })
            .map(|(path, manifest, new_content)| {
                let source = manifest.package.map_or_else(
                    || EditSource::WorkspaceDependencies {
                        dependencies: manifest.dependencies.into_iter().collect(),
                    },
                    |package| EditSource::PackageVersion { package },
                );
                Ok(FileEdit {
                    path: path.into(),
                    expected: FileEditExpectation::Existing {
                        hash: FileHash::from_bytes(manifest.original.as_bytes()),
                    },
                    new_content,
                    source,
                })
            })
            .collect()
    }

    fn manifest_path(package: &PackageSnapshot) -> String {
        if package.path.as_str().is_empty() || package.path == "." {
            "Cargo.toml".to_string()
        } else {
            format!("{}/Cargo.toml", package.path.as_str().trim_end_matches('/'))
        }
    }

    fn load_manifest<'manifests>(
        root: &Path,
        relative_path: &str,
        manifests: &'manifests mut BTreeMap<String, PlannedManifest>,
    ) -> Result<&'manifests mut PlannedManifest, ResolveError> {
        if !manifests.contains_key(relative_path) {
            let absolute_path = root.join(relative_path);
            let original = std::fs::read_to_string(&absolute_path)?;
            let document = original
                .parse::<toml_edit::DocumentMut>()
                .map_err(|error| ResolveError::ParseError {
                    path: absolute_path,
                    reason: error.to_string(),
                })?;
            manifests.insert(
                relative_path.to_string(),
                PlannedManifest {
                    original,
                    document,
                    package: None,
                    dependencies: BTreeSet::new(),
                },
            );
        }
        Ok(manifests
            .get_mut(relative_path)
            .expect("the Rust manifest is inserted immediately before this lookup"))
    }

    fn update_dependency_versions(
        dependencies: &mut toml_edit::Table,
        changed_versions: &BTreeMap<String, (PackageId, String)>,
    ) -> BTreeSet<PackageId> {
        let mut updated = BTreeSet::new();
        for (name, dependency) in dependencies.iter_mut() {
            let manifest_name = dependency
                .get("package")
                .and_then(toml_edit::Item::as_str)
                .unwrap_or(name.get());
            let Some((package, version)) = changed_versions.get(manifest_name) else {
                continue;
            };
            if dependency.is_str() {
                if dependency.as_str() != Some(version.as_str()) {
                    *dependency = toml_edit::value(version);
                    updated.insert(package.clone());
                }
            } else if dependency.get("version").is_some()
                && dependency.get("version").and_then(toml_edit::Item::as_str)
                    != Some(version.as_str())
            {
                dependency["version"] = toml_edit::value(version);
                updated.insert(package.clone());
            }
        }
        updated
    }

    fn dependency_requirement(dependency: &serde_json::Value) -> Option<String> {
        dependency
            .as_str()
            .or_else(|| {
                dependency
                    .get("version")
                    .and_then(serde_json::Value::as_str)
            })
            .map(str::to_string)
    }

    fn collect_dependencies(
        dependencies: Option<BTreeMap<String, serde_json::Value>>,
        workspace_dependencies: Option<&BTreeMap<String, serde_json::Value>>,
        kind: DependencyKind,
    ) -> Vec<ManifestDependency> {
        dependencies
            .unwrap_or_default()
            .into_iter()
            .map(|(name, dependency)| {
                let dependency = dependency
                    .get("workspace")
                    .and_then(serde_json::Value::as_bool)
                    .filter(|workspace| *workspace)
                    .and_then(|_| {
                        workspace_dependencies.and_then(|dependencies| dependencies.get(&name))
                    })
                    .unwrap_or(&dependency);
                ManifestDependency {
                    manifest_name: dependency
                        .get("package")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(&name)
                        .to_string(),
                    kind,
                    requirement: Self::dependency_requirement(dependency),
                }
            })
            .collect()
    }

    pub fn manifest_dependencies(
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<Vec<ManifestDependency>, ResolveError> {
        let cargo_toml_path = root.join(&pkg_config.path).join("Cargo.toml");
        let cargo_toml: CargoToml =
            toml_edit::de::from_str(&std::fs::read_to_string(&cargo_toml_path)?).map_err(|e| {
                ResolveError::ParseError {
                    path: cargo_toml_path,
                    reason: e.to_string(),
                }
            })?;
        let workspace_manifest_path = root.join("Cargo.toml");
        let workspace_dependencies = if workspace_manifest_path.exists() {
            let workspace_manifest: CargoToml =
                toml_edit::de::from_str(&std::fs::read_to_string(&workspace_manifest_path)?)
                    .map_err(|e| ResolveError::ParseError {
                        path: workspace_manifest_path,
                        reason: e.to_string(),
                    })?;
            workspace_manifest
                .workspace
                .and_then(|workspace| workspace.dependencies)
        } else {
            None
        };

        Ok([
            Self::collect_dependencies(
                cargo_toml.dependencies,
                workspace_dependencies.as_ref(),
                DependencyKind::Runtime,
            ),
            Self::collect_dependencies(
                cargo_toml.dev_dependencies,
                workspace_dependencies.as_ref(),
                DependencyKind::Development,
            ),
            Self::collect_dependencies(
                cargo_toml.build_dependencies,
                workspace_dependencies.as_ref(),
                DependencyKind::Build,
            ),
        ]
        .concat())
    }
}

impl EcosystemAdapter for RustResolver {
    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Rust
    }

    fn encode_version(&self, version: &semver::Version) -> Result<String, AdapterError> {
        Ok(version.to_string())
    }

    fn discover(&self, root: &camino::Utf8Path) -> Result<Vec<PackageInspection>, AdapterError> {
        let packages = self.discover_packages(root.as_std_path())?;
        let mut inspections = packages
            .into_iter()
            .map(|package| {
                let dependencies = Self::manifest_dependencies(
                    root.as_std_path(),
                    &Self::package_config(package.path.clone()),
                )?;
                Self::package_inspection(
                    PackageId::new(package.name.clone()),
                    package,
                    dependencies,
                )
            })
            .collect::<Result<Vec<_>, AdapterError>>()?;
        inspections.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(inspections)
    }

    fn inspect(&self, location: &PackageLocation) -> Result<PackageInspection, AdapterError> {
        if location.path.is_absolute()
            || location
                .path
                .components()
                .any(|component| component == camino::Utf8Component::ParentDir)
        {
            return Err(AdapterError::InvalidInput {
                reason: format!(
                    "Rust package path must be relative to the project root: {}",
                    location.path
                ),
            });
        }
        let config = Self::package_config(location.path.as_std_path());
        let package = self.parse_package(location.project_root.as_std_path(), &config)?;
        let dependencies =
            Self::manifest_dependencies(location.project_root.as_std_path(), &config)?;
        Self::package_inspection(location.id.clone(), package, dependencies)
    }

    fn plan_edits(&self, input: EcosystemPlanInput<'_>) -> Result<Vec<FileEdit>, AdapterError> {
        if input
            .workspace_packages
            .iter()
            .any(|package| package.ecosystem != Ecosystem::Rust)
        {
            return Err(AdapterError::InvalidInput {
                reason: "Rust edit planning received a non-Rust workspace package".to_string(),
            });
        }
        let workspace_packages = input
            .workspace_packages
            .iter()
            .map(|package| (package.id.clone(), package))
            .collect::<BTreeMap<_, _>>();
        let released_packages = input
            .released_packages
            .iter()
            .map(|id| {
                workspace_packages
                    .get(id)
                    .copied()
                    .ok_or_else(|| AdapterError::InvalidInput {
                        reason: format!("released Rust package {id} is not in the workspace"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let workspace_package_refs = input.workspace_packages.iter().collect::<Vec<_>>();
        Ok(Self::plan_file_edits(
            input.project_root.as_std_path(),
            &released_packages,
            &workspace_package_refs,
            input.versions,
        )?)
    }
}

impl RustResolver {
    fn parse_package(
        &self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<ParsedPackage, ResolveError> {
        let toml_path = root.join(&pkg_config.path).join("Cargo.toml");
        if !toml_path.exists() {
            return Err(ResolveError::FileOrDirNotFound {
                path: toml_path.clone(),
            });
        }
        let toml_str = std::fs::read_to_string(&toml_path)?;
        let cargo_toml: CargoToml =
            toml_edit::de::from_str(&toml_str).map_err(|e| ResolveError::ParseError {
                path: toml_path.clone(),
                reason: e.to_string(),
            })?;
        let cargo_pkg_config = cargo_toml.package.ok_or(ResolveError::InvalidConfig {
            path: toml_path.clone(),
            reason: "Not found package in Cargo.toml".into(),
        })?;
        let publish = cargo_pkg_config.publish.unwrap_or(true);
        let package = ParsedPackage {
            name: cargo_pkg_config.name,
            version: semver::Version::parse(&cargo_pkg_config.version)?,
            path: pkg_config.path.clone(),
            private: !publish,
        };
        Ok(package)
    }

    fn discover_packages(&self, root: &Path) -> Result<Vec<ParsedPackage>, ResolveError> {
        let cargo_toml_path = root.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            log::warn!(
                "Cannot resolve package in {}, Cargo.toml not found.",
                root.display()
            );
            return Ok(vec![]);
        }

        let toml_str = std::fs::read_to_string(&cargo_toml_path)?;
        let cargo_toml: CargoToml =
            toml_edit::de::from_str(&toml_str).map_err(|e| ResolveError::ParseError {
                path: cargo_toml_path.clone(),
                reason: e.to_string(),
            })?;

        if cargo_toml.workspace.is_none() {
            if cargo_toml.package.is_none() {
                log::warn!("Failed to resolve package in {}", root.display());
                return Ok(vec![]);
            }
            let package = self.parse_package(
                root,
                &PackageConfig {
                    path: ".".into(),
                    resolver: ResolverType::Rust,
                    channel: ReleaseChannel::Stable,
                    assets: vec![],
                    depends_on: vec![],
                },
            )?;
            return Ok(vec![package]);
        }

        let workspace = cargo_toml
            .workspace
            .expect("workspace presence was checked above");
        let members = workspace
            .members
            .iter()
            .try_fold(Vec::new(), |mut members, member| {
                let pattern = root.join(member).display().to_string();
                let paths = glob::glob(&pattern)
                    .map_err(|e| ResolveError::ParseError {
                        path: cargo_toml_path.clone(),
                        reason: e.to_string(),
                    })?
                    .map(|path| {
                        path.map_err(|error| ResolveError::ParseError {
                            path: error.path().to_path_buf(),
                            reason: error.to_string(),
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                members.extend(paths);
                Ok::<_, ResolveError>(members)
            })?;

        log::debug!("members: {members:?}");

        let packages = members
            .into_iter()
            .map(|path| {
                let rel_path = pathdiff::diff_paths(&path, root).unwrap_or(path);
                self.parse_package(
                    root,
                    &PackageConfig {
                        path: rel_path.to_path_buf(),
                        resolver: ResolverType::Rust,
                        channel: ReleaseChannel::Stable,
                        assets: vec![],
                        depends_on: vec![],
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(packages)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        adapter::{EcosystemAdapter, EcosystemPlanInput, PackageLocation},
        config::{PackageConfig, ReleaseChannel},
        resolver::ResolverType,
    };
    use semifold_core::{Ecosystem, EditSource, PackageId, PackageSnapshot, VersionMap};

    use super::RustResolver;

    fn temp_dir(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "semifold-rust-resolver-{test_name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn package_config(path: impl Into<PathBuf>) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Rust,
            channel: ReleaseChannel::Stable,
            assets: vec![],
            depends_on: vec![],
        }
    }

    fn write_package(
        root: &Path,
        path: &str,
        name: &str,
        version: &str,
        publish: Option<bool>,
        dependencies: Option<&str>,
    ) {
        let package_root = root.join(path);
        fs::create_dir_all(&package_root).unwrap();
        let publish = publish
            .map(|value| format!("publish = {value}\n"))
            .unwrap_or_default();
        let dependencies = dependencies
            .map(|value| format!("\n[dependencies]\n{value}\n"))
            .unwrap_or_default();
        fs::write(
            package_root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"{version}\"\n{publish}{dependencies}"
            ),
        )
        .unwrap();
    }

    #[test]
    fn resolves_a_single_package() {
        let root = temp_dir("single-package");
        write_package(&root, ".", "single", "1.2.3", None, None);

        let package = RustResolver
            .parse_package(&root, &package_config("."))
            .unwrap();

        assert_eq!(package.name, "single");
        assert_eq!(package.version, semver::Version::parse("1.2.3").unwrap());
        assert!(!package.private);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_workspace_members_and_private_packages() {
        let root = temp_dir("workspace");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        write_package(&root, "crates/core", "core", "1.0.0", None, None);
        write_package(
            &root,
            "crates/internal",
            "internal",
            "1.0.0",
            Some(false),
            None,
        );

        let mut packages = RustResolver.discover_packages(&root).unwrap();
        packages.sort_by(|left, right| left.name.cmp(&right.name));

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "core");
        assert_eq!(packages[0].path, PathBuf::from("crates/core"));
        assert!(!packages[0].private);
        assert_eq!(packages[1].name, "internal");
        assert_eq!(packages[1].path, PathBuf::from("crates/internal"));
        assert!(packages[1].private);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_discovers_and_inspects_manifest_dependencies_before_id_binding() {
        let root = temp_dir("adapter-inspection");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        write_package(&root, "crates/core", "core", "1.0.0", None, None);
        write_package(
            &root,
            "crates/app",
            "app",
            "1.0.0",
            None,
            Some("core = { version = \"1\", path = \"../core\" }\nserde = \"1\""),
        );
        let project_root = camino::Utf8PathBuf::from_path_buf(root.clone()).unwrap();

        let discovered = RustResolver.discover(&project_root).unwrap();
        assert_eq!(
            discovered
                .iter()
                .map(|package| package.id.as_str())
                .collect::<Vec<_>>(),
            ["app", "core"]
        );
        let app = RustResolver
            .inspect(&PackageLocation {
                id: PackageId::new("configured-app"),
                project_root,
                path: "crates/app".into(),
            })
            .unwrap();

        assert_eq!(app.id, PackageId::new("configured-app"));
        assert_eq!(app.manifest_name, "app");
        assert_eq!(
            app.dependencies
                .iter()
                .map(|dependency| dependency.manifest_name.as_str())
                .collect::<Vec<_>>(),
            ["core", "serde"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plans_a_manifest_edit_from_the_complete_version_map() {
        let root = temp_dir("plan-file-edit");
        let manifest_path = root.join("crates/app/Cargo.toml");
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        let original = "# keep this comment\n[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\ncore = { version = \"1.0.0\", path = \"../core\", features = [\"serde\"] }\nalias = { package = \"renamed\", version = \"2.0.0\" }\n\n[dev-dependencies]\ndev = \"3.0.0\"\n\n[build-dependencies]\nbuild = { version = \"4.0.0\" }\n";
        fs::write(&manifest_path, original).unwrap();
        let package = PackageSnapshot {
            id: PackageId::new("app"),
            manifest_name: "app".to_string(),
            version: semver::Version::new(1, 0, 0),
            ecosystem: Ecosystem::Rust,
            path: "crates/app".into(),
            publishable: true,
            dependencies: vec![],
        };
        let internal = [
            package.clone(),
            PackageSnapshot {
                id: PackageId::new("core-id"),
                manifest_name: "core".to_string(),
                version: semver::Version::new(1, 0, 0),
                ecosystem: Ecosystem::Rust,
                path: "crates/core".into(),
                publishable: true,
                dependencies: vec![],
            },
            PackageSnapshot {
                id: PackageId::new("renamed-id"),
                manifest_name: "renamed".to_string(),
                version: semver::Version::new(2, 0, 0),
                ecosystem: Ecosystem::Rust,
                path: "crates/renamed".into(),
                publishable: true,
                dependencies: vec![],
            },
            PackageSnapshot {
                id: PackageId::new("dev-id"),
                manifest_name: "dev".to_string(),
                version: semver::Version::new(3, 0, 0),
                ecosystem: Ecosystem::Rust,
                path: "crates/dev".into(),
                publishable: true,
                dependencies: vec![],
            },
            PackageSnapshot {
                id: PackageId::new("build-id"),
                manifest_name: "build".to_string(),
                version: semver::Version::new(4, 0, 0),
                ecosystem: Ecosystem::Rust,
                path: "crates/build".into(),
                publishable: true,
                dependencies: vec![],
            },
        ];
        let versions = VersionMap::from([
            (PackageId::new("app"), semver::Version::new(1, 0, 1)),
            (PackageId::new("core-id"), semver::Version::new(1, 1, 0)),
            (PackageId::new("renamed-id"), semver::Version::new(2, 1, 0)),
            (PackageId::new("dev-id"), semver::Version::new(3, 1, 0)),
            (PackageId::new("build-id"), semver::Version::new(4, 1, 0)),
        ]);

        let project_root = camino::Utf8Path::from_path(&root).unwrap();
        let edits = RustResolver
            .plan_edits(EcosystemPlanInput {
                project_root,
                workspace_packages: &internal,
                released_packages: std::slice::from_ref(&package.id),
                versions: &versions,
            })
            .unwrap();
        let edit = &edits[0];

        assert_eq!(edit.path.as_str(), "crates/app/Cargo.toml");
        assert_eq!(
            edit.expected,
            semifold_core::FileEditExpectation::Existing {
                hash: semifold_core::FileHash::from_bytes(original.as_bytes()),
            }
        );
        assert!(edit.new_content.contains("# keep this comment"));
        assert!(edit.new_content.contains("version = \"1.0.1\""));
        assert!(edit.new_content.contains(
            "core = { version = \"1.1.0\", path = \"../core\", features = [\"serde\"] }"
        ));
        assert!(
            edit.new_content
                .contains("alias = { package = \"renamed\", version = \"2.1.0\" }")
        );
        assert!(edit.new_content.contains("dev = \"3.1.0\""));
        assert!(edit.new_content.contains("build = { version = \"4.1.0\" }"));
        assert_eq!(fs::read_to_string(manifest_path).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn merges_workspace_dependency_updates_independently_of_release_order() {
        let root = temp_dir("workspace-edit-plan");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.dependencies]\ncore-alias = { package = \"core\", version = \"1.0.0\", path = \"crates/core\" }\nhelper = \"2.0.0\"\nexternal = \"^9\"\n",
        )
        .unwrap();
        write_package(&root, "crates/core", "core", "1.0.0", None, None);
        write_package(&root, "crates/helper", "helper", "2.0.0", None, None);
        let app_manifest = root.join("crates/app/Cargo.toml");
        fs::create_dir_all(app_manifest.parent().unwrap()).unwrap();
        fs::write(
            &app_manifest,
            "[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\ncore-alias = { workspace = true }\n\n[dev-dependencies]\nhelper = { workspace = true }\n",
        )
        .unwrap();

        let app = PackageSnapshot {
            id: PackageId::new("app-id"),
            manifest_name: "app".to_string(),
            version: semver::Version::new(1, 0, 0),
            ecosystem: Ecosystem::Rust,
            path: "crates/app".into(),
            publishable: true,
            dependencies: vec![],
        };
        let core = PackageSnapshot {
            id: PackageId::new("core-id"),
            manifest_name: "core".to_string(),
            version: semver::Version::new(1, 0, 0),
            ecosystem: Ecosystem::Rust,
            path: "crates/core".into(),
            publishable: true,
            dependencies: vec![],
        };
        let helper = PackageSnapshot {
            id: PackageId::new("helper-id"),
            manifest_name: "helper".to_string(),
            version: semver::Version::new(2, 0, 0),
            ecosystem: Ecosystem::Rust,
            path: "crates/helper".into(),
            publishable: true,
            dependencies: vec![],
        };
        let versions = VersionMap::from([
            (PackageId::new("app-id"), semver::Version::new(1, 0, 1)),
            (PackageId::new("core-id"), semver::Version::new(1, 1, 0)),
            (PackageId::new("helper-id"), semver::Version::new(2, 1, 0)),
        ]);
        let workspace = [&app, &core, &helper];

        let first =
            RustResolver::plan_file_edits(&root, &[&app, &core, &helper], &workspace, &versions)
                .unwrap();
        let second =
            RustResolver::plan_file_edits(&root, &[&helper, &core, &app], &workspace, &versions)
                .unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .filter(|edit| edit.path == "Cargo.toml")
                .count(),
            1
        );
        let workspace_edit = first.iter().find(|edit| edit.path == "Cargo.toml").unwrap();
        assert!(workspace_edit.new_content.contains(
            "core-alias = { package = \"core\", version = \"1.1.0\", path = \"crates/core\" }"
        ));
        assert!(workspace_edit.new_content.contains("helper = \"2.1.0\""));
        assert!(workspace_edit.new_content.contains("external = \"^9\""));
        assert!(matches!(
            &workspace_edit.source,
            EditSource::WorkspaceDependencies { dependencies }
                if dependencies
                    == &[PackageId::new("core-id"), PackageId::new("helper-id")]
        ));
        let app_edit = first
            .iter()
            .find(|edit| edit.path == "crates/app/Cargo.toml")
            .unwrap();
        assert!(
            app_edit
                .new_content
                .contains("core-alias = { workspace = true }")
        );
        assert!(
            app_edit
                .new_content
                .contains("helper = { workspace = true }")
        );
        assert!(
            fs::read_to_string(root.join("Cargo.toml"))
                .unwrap()
                .contains("version = \"1.0.0\"")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
