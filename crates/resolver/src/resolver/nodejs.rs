use std::{collections::BTreeMap, path::Path};

use saphyr::LoadableYamlNode;
use semifold_core::{
    DependencyKind, Ecosystem, EditSource, FileEdit, FileEditExpectation, FileHash, PackageId,
    PackageSnapshot, VersionMap,
};
use serde::Deserialize;

use crate::{
    adapter::{
        AdapterError, EcosystemAdapter, EcosystemPlanInput, ManifestDependency, PackageInspection,
        PackageLocation,
    },
    config::{PackageConfig, ReleaseChannel, ResolverConfig},
    error::ResolveError,
    resolver::{ResolvedDependency, ResolvedPackage, Resolver, ResolverType},
    utils,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageJson {
    pub name: String,
    #[serde(default = "default_node_version")]
    pub version: String,
    pub workspaces: Option<Vec<String>>,
    pub dependencies: Option<BTreeMap<String, String>>,
    pub dev_dependencies: Option<BTreeMap<String, String>>,
    pub peer_dependencies: Option<BTreeMap<String, String>>,
    pub optional_dependencies: Option<BTreeMap<String, String>>,
    pub private: Option<bool>,
}

pub struct NodejsResolver;

fn default_node_version() -> String {
    "0.0.0".to_string()
}

impl NodejsResolver {
    fn package_config(path: impl Into<std::path::PathBuf>) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Nodejs,
            channel: ReleaseChannel::Stable,
            assets: Vec::new(),
            depends_on: vec![],
        }
    }

    fn package_inspection(
        id: PackageId,
        package: ResolvedPackage,
        dependencies: Vec<ResolvedDependency>,
    ) -> Result<PackageInspection, AdapterError> {
        let path = camino::Utf8PathBuf::from_path_buf(package.path).map_err(|path| {
            AdapterError::InvalidInput {
                reason: format!(
                    "Node.js package path is not valid UTF-8: {}",
                    path.display()
                ),
            }
        })?;
        Ok(PackageInspection {
            id,
            manifest_name: package.name,
            version: package.version,
            ecosystem: Ecosystem::Node,
            path,
            publishable: !package.private,
            dependencies: dependencies
                .into_iter()
                .map(|dependency| ManifestDependency {
                    manifest_name: dependency.manifest_name,
                    kind: dependency.kind,
                    requirement: dependency.requirement,
                })
                .collect(),
        })
    }

    /// Plans a package.json replacement from immutable package and version snapshots.
    pub fn plan_file_edit(
        root: &Path,
        package: &PackageSnapshot,
        versions: &VersionMap,
    ) -> Result<FileEdit, ResolveError> {
        let manifest_versions = versions
            .iter()
            .map(|(package, version)| (package.to_string(), version.clone()))
            .collect();
        Self::plan_package_edit(root, package, versions, &manifest_versions)
    }

    fn plan_package_edit(
        root: &Path,
        package: &PackageSnapshot,
        versions: &VersionMap,
        manifest_versions: &BTreeMap<String, semver::Version>,
    ) -> Result<FileEdit, ResolveError> {
        let package_json_path = root.join(package.path.as_std_path()).join("package.json");
        let original = std::fs::read_to_string(&package_json_path)?;
        let next_version =
            versions
                .get(&package.id)
                .ok_or_else(|| ResolveError::InvalidConfig {
                    path: package_json_path.clone(),
                    reason: format!("missing planned version for {}", package.id),
                })?;
        let _: PackageJson =
            serde_json::from_str(&original).map_err(|error| ResolveError::ParseError {
                path: package_json_path.clone(),
                reason: error.to_string(),
            })?;
        let mut document: serde_json::Value =
            serde_json::from_str(&original).map_err(|error| ResolveError::ParseError {
                path: package_json_path.clone(),
                reason: error.to_string(),
            })?;
        let object = document.as_object_mut().ok_or(ResolveError::ParseError {
            path: package_json_path.clone(),
            reason: "package.json root must be an object".to_string(),
        })?;
        object.insert(
            "version".to_string(),
            serde_json::Value::String(next_version.to_string()),
        );
        for field in [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ] {
            let Some(dependencies) = object
                .get_mut(field)
                .and_then(serde_json::Value::as_object_mut)
            else {
                continue;
            };
            for (name, requirement_value) in dependencies {
                let Some(requirement) = requirement_value.as_str() else {
                    continue;
                };
                let Some(version) = manifest_versions.get(name) else {
                    continue;
                };
                let next_requirement = node_requirement(requirement, version);
                *requirement_value = serde_json::Value::String(next_requirement);
            }
        }
        let mut updated =
            serde_json::to_string_pretty(&document).map_err(|error| ResolveError::ParseError {
                path: package_json_path.clone(),
                reason: error.to_string(),
            })?;
        updated.push('\n');

        Ok(FileEdit {
            path: package.path.join("package.json"),
            expected: FileEditExpectation::Existing {
                hash: FileHash::from_bytes(original.as_bytes()),
            },
            new_content: updated,
            source: EditSource::PackageVersion {
                package: package.id.clone(),
            },
        })
    }

    fn collect_dependencies(
        target: &mut Vec<ResolvedDependency>,
        dependencies: Option<BTreeMap<String, String>>,
        kind: DependencyKind,
    ) {
        target.extend(dependencies.unwrap_or_default().into_iter().map(
            |(manifest_name, requirement)| ResolvedDependency {
                manifest_name,
                kind,
                requirement: Some(requirement),
            },
        ));
    }

    fn manifest_dependencies(
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<Vec<ResolvedDependency>, ResolveError> {
        let package_json_path = root.join(&pkg_config.path).join("package.json");
        let package_json: PackageJson =
            serde_json::from_str(&std::fs::read_to_string(&package_json_path)?).map_err(|e| {
                ResolveError::ParseError {
                    path: package_json_path,
                    reason: e.to_string(),
                }
            })?;
        let mut dependencies = Vec::new();
        Self::collect_dependencies(
            &mut dependencies,
            package_json.dependencies,
            DependencyKind::Runtime,
        );
        Self::collect_dependencies(
            &mut dependencies,
            package_json.dev_dependencies,
            DependencyKind::Development,
        );
        Self::collect_dependencies(
            &mut dependencies,
            package_json.peer_dependencies,
            DependencyKind::Peer,
        );
        Self::collect_dependencies(
            &mut dependencies,
            package_json.optional_dependencies,
            DependencyKind::Optional,
        );
        Ok(dependencies)
    }
}

fn node_requirement(requirement: &str, version: &semver::Version) -> String {
    if requirement == "workspace:*" {
        return requirement.to_string();
    }
    let version = version.to_string();
    for prefix in ["workspace:^", "workspace:~", "^", "~"] {
        if requirement.starts_with(prefix) {
            return format!("{prefix}{version}");
        }
    }
    version
}

impl EcosystemAdapter for NodejsResolver {
    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Node
    }

    fn discover(&self, root: &camino::Utf8Path) -> Result<Vec<PackageInspection>, AdapterError> {
        let mut resolver = Self;
        let packages = resolver.resolve_all(root.as_std_path())?;
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
                    "Node.js package path must be relative to the project root: {}",
                    location.path
                ),
            });
        }
        let config = Self::package_config(location.path.as_std_path());
        let mut resolver = Self;
        let package = resolver.resolve(location.project_root.as_std_path(), &config)?;
        let dependencies =
            Self::manifest_dependencies(location.project_root.as_std_path(), &config)?;
        Self::package_inspection(location.id.clone(), package, dependencies)
    }

    fn plan_edits(&self, input: EcosystemPlanInput<'_>) -> Result<Vec<FileEdit>, AdapterError> {
        if input
            .workspace_packages
            .iter()
            .any(|package| package.ecosystem != Ecosystem::Node)
        {
            return Err(AdapterError::InvalidInput {
                reason: "Node.js edit planning received a non-Node workspace package".to_string(),
            });
        }
        let workspace_packages = input
            .workspace_packages
            .iter()
            .map(|package| (package.id.clone(), package))
            .collect::<BTreeMap<_, _>>();
        let manifest_versions = input
            .workspace_packages
            .iter()
            .filter_map(|package| {
                input
                    .versions
                    .get(&package.id)
                    .filter(|version| *version != &package.version)
                    .map(|version| (package.manifest_name.clone(), version.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let released_packages = input
            .released_packages
            .iter()
            .collect::<std::collections::BTreeSet<_>>();

        released_packages
            .into_iter()
            .map(|id| {
                let package = workspace_packages.get(id).copied().ok_or_else(|| {
                    AdapterError::InvalidInput {
                        reason: format!("released Node.js package {id} is not in the workspace"),
                    }
                })?;
                Ok(Self::plan_package_edit(
                    input.project_root.as_std_path(),
                    package,
                    input.versions,
                    &manifest_versions,
                )?)
            })
            .collect()
    }
}

impl Resolver for NodejsResolver {
    fn resolve(
        &mut self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<ResolvedPackage, ResolveError> {
        let package_json_path = root.join(&pkg_config.path).join("package.json");
        if !package_json_path.exists() {
            return Err(ResolveError::FileOrDirNotFound {
                path: package_json_path.clone(),
            });
        }
        let package_json_str = std::fs::read_to_string(&package_json_path)?;
        let package_json: PackageJson =
            serde_json::from_str(&package_json_str).map_err(|e| ResolveError::ParseError {
                path: package_json_path.clone(),
                reason: e.to_string(),
            })?;

        let package = ResolvedPackage {
            name: package_json.name,
            version: semver::Version::parse(&package_json.version)?,
            path: pkg_config.path.clone(),
            private: package_json.private.unwrap_or(false),
        };
        Ok(package)
    }

    fn resolve_all(&mut self, root: &Path) -> Result<Vec<ResolvedPackage>, ResolveError> {
        let package_json_path = root.join("package.json");
        if !package_json_path.exists() {
            log::warn!(
                "Cannot resolve package in {}, package.json not found.",
                root.display()
            );
            return Ok(vec![]);
        }

        let package_json_str = std::fs::read_to_string(&package_json_path)?;
        let package_json: PackageJson =
            serde_json::from_str(&package_json_str).map_err(|e| ResolveError::ParseError {
                path: package_json_path.clone(),
                reason: e.to_string(),
            })?;

        let pnpm_workspace_file_path = root.join("pnpm-workspace.yaml");
        let pnpm_packages = if pnpm_workspace_file_path.exists() {
            let pnpm_workspace_yaml =
                saphyr::Yaml::load_from_str(&std::fs::read_to_string(&pnpm_workspace_file_path)?)
                    .map_err(|e| ResolveError::ParseError {
                    path: pnpm_workspace_file_path.clone(),
                    reason: e.to_string(),
                })?;
            pnpm_workspace_yaml
                .first()
                .and_then(|yaml| yaml.as_mapping_get("packages"))
                .and_then(|yaml| yaml.as_vec())
                .map(|vec| {
                    vec.iter()
                        .map(|item| item.as_str().unwrap_or_default().to_string())
                        .collect::<Vec<_>>()
                })
        } else {
            None
        };
        let Some(workspaces) = pnpm_packages.or(package_json.workspaces) else {
            if package_json.name.is_empty() {
                log::warn!("Failed to resolve package in {}", root.display());
                return Ok(vec![]);
            }
            let package = self.resolve(
                root,
                &PackageConfig {
                    path: ".".into(),
                    resolver: ResolverType::Nodejs,
                    channel: ReleaseChannel::Stable,
                    assets: vec![],
                    depends_on: vec![],
                },
            )?;
            return Ok(vec![package]);
        };
        let mut packages = Vec::new();

        let root_package = self.resolve(
            root,
            &PackageConfig {
                path: ".".into(),
                resolver: ResolverType::Nodejs,
                channel: ReleaseChannel::Stable,
                assets: vec![],
                depends_on: vec![],
            },
        )?;
        packages.push(root_package);

        for workspace_pattern in workspaces {
            let pattern = root.join(&workspace_pattern).display().to_string();
            let paths = glob::glob(&pattern)
                .map_err(|e| ResolveError::ParseError {
                    path: package_json_path.clone(),
                    reason: e.to_string(),
                })?
                .map(|path| {
                    path.map_err(|error| ResolveError::ParseError {
                        path: error.path().to_path_buf(),
                        reason: error.to_string(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            for path in paths {
                if path == root {
                    continue;
                }

                if path.join("package.json").exists() {
                    let rel_path = pathdiff::diff_paths(&path, root).unwrap_or(path.clone());
                    packages.push(self.resolve(
                        root,
                        &PackageConfig {
                            path: rel_path,
                            resolver: ResolverType::Nodejs,
                            channel: ReleaseChannel::Stable,
                            assets: vec![],
                            depends_on: vec![],
                        },
                    )?);
                }
            }
        }

        Ok(packages)
    }

    fn dependencies(
        &mut self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<Vec<ResolvedDependency>, ResolveError> {
        Self::manifest_dependencies(root, pkg_config)
    }

    fn publish(
        &mut self,
        package: &ResolvedPackage,
        resolver_config: &ResolverConfig,
        dry_run: bool,
    ) -> Result<(), ResolveError> {
        if package.private {
            log::warn!(
                "Skip publish {} {} due to private flag",
                package.name,
                format_args!("v{}", package.version)
            );
            return Ok(());
        }

        log::info!("Running prepublish commands for {}", package.name);
        for prepublish in &resolver_config.prepublish {
            let args = prepublish.args.clone().unwrap_or_default();
            if dry_run && !prepublish.dry_run.unwrap_or(false) {
                log::warn!(
                    "Skip prepublish command {} {} due to dry run",
                    prepublish.command,
                    args.join(" ")
                );
                continue;
            }
            log::info!("Running {} {}", prepublish.command, args.join(" "));
            utils::run_command(prepublish, &package.path)?;
        }

        log::info!("Running publish commands for {}", package.name);
        for publish in &resolver_config.publish {
            let args = publish.args.clone().unwrap_or_default();
            if dry_run && !publish.dry_run.unwrap_or(false) {
                log::warn!(
                    "Skip publish command {} {} due to dry run",
                    publish.command,
                    args.join(" ")
                );
                continue;
            }
            log::info!("Running {} {}", publish.command, args.join(" "));
            utils::run_command(publish, &package.path)?;
        }

        Ok(())
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
        resolver::{Resolver, ResolverType},
    };
    use semifold_core::{Ecosystem, PackageId, PackageSnapshot, VersionMap};

    use super::NodejsResolver;

    fn temp_dir(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "semifold-nodejs-resolver-{test_name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn package_config(path: impl Into<PathBuf>) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Nodejs,
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
        private: bool,
        extra: &str,
    ) {
        let package_root = root.join(path);
        fs::create_dir_all(&package_root).unwrap();
        fs::write(
            package_root.join("package.json"),
            format!(
                "{{\n  \"name\": \"{name}\",\n  \"version\": \"{version}\",\n  \"private\": {private}{extra}\n}}\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn resolves_a_single_package() {
        let root = temp_dir("single-package");
        write_package(&root, ".", "single", "1.2.3", false, "");

        let package = NodejsResolver.resolve(&root, &package_config(".")).unwrap();

        assert_eq!(package.name, "single");
        assert_eq!(package.version, semver::Version::parse("1.2.3").unwrap());
        assert!(!package.private);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_a_package_without_a_version_as_zero() {
        let root = temp_dir("missing-version");
        fs::write(
            root.join("package.json"),
            "{\n  \"name\": \"template\"\n}\n",
        )
        .unwrap();

        let package = NodejsResolver.resolve(&root, &package_config(".")).unwrap();

        assert_eq!(package.name, "template");
        assert_eq!(package.version, semver::Version::new(0, 0, 0));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_pnpm_workspace_packages_and_private_members() {
        let root = temp_dir("pnpm-workspace");
        write_package(
            &root,
            ".",
            "root",
            "1.0.0",
            true,
            ",\n  \"workspaces\": [\"ignored/*\"]",
        );
        fs::write(
            root.join("pnpm-workspace.yaml"),
            "packages:\n  - 'packages/*'\n",
        )
        .unwrap();
        write_package(&root, "packages/core", "core", "1.0.0", false, "");
        write_package(&root, "packages/internal", "internal", "1.0.0", true, "");

        let mut packages = NodejsResolver.resolve_all(&root).unwrap();
        packages.sort_by(|left, right| left.name.cmp(&right.name));

        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "core");
        assert_eq!(packages[0].path, PathBuf::from("packages/core"));
        assert!(!packages[0].private);
        assert_eq!(packages[1].name, "internal");
        assert_eq!(packages[1].path, PathBuf::from("packages/internal"));
        assert!(packages[1].private);
        assert_eq!(packages[2].name, "root");
        assert_eq!(packages[2].path, PathBuf::from("."));
        assert!(packages[2].private);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_discovers_and_inspects_manifest_dependencies_before_id_binding() {
        let root = temp_dir("adapter-inspection");
        write_package(
            &root,
            ".",
            "root",
            "1.0.0",
            true,
            ",\n  \"workspaces\": [\"packages/*\"]",
        );
        write_package(&root, "packages/core", "core", "1.0.0", false, "");
        write_package(
            &root,
            "packages/app",
            "app",
            "1.0.0",
            false,
            ",\n  \"dependencies\": { \"core\": \"^1\", \"react\": \"^19\" }",
        );
        let project_root = camino::Utf8PathBuf::from_path_buf(root.clone()).unwrap();

        let discovered = NodejsResolver.discover(&project_root).unwrap();
        assert_eq!(
            discovered
                .iter()
                .map(|package| package.id.as_str())
                .collect::<Vec<_>>(),
            ["app", "core", "root"]
        );
        let app = NodejsResolver
            .inspect(&PackageLocation {
                id: PackageId::new("configured-app"),
                project_root,
                path: "packages/app".into(),
            })
            .unwrap();

        assert_eq!(app.id, PackageId::new("configured-app"));
        assert_eq!(app.manifest_name, "app");
        assert_eq!(
            app.dependencies
                .iter()
                .map(|dependency| dependency.manifest_name.as_str())
                .collect::<Vec<_>>(),
            ["core", "react"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plans_a_package_json_edit_from_the_complete_version_map() {
        let root = temp_dir("plan-file-edit");
        let manifest_path = root.join("packages/app/package.json");
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        let original = r#"{
  "name": "app",
  "dependencies": { "core": "^1.0.0", "workspace": "workspace:*" },
  "devDependencies": { "dev": "~2.0.0" },
  "peerDependencies": { "peer": "workspace:^3.0.0" },
  "optionalDependencies": { "optional": "4.0.0" },
  "custom": { "preserved": true, "escaped": "quote \\\" and slash \\\\" }
}
"#;
        fs::write(&manifest_path, original).unwrap();
        let package = PackageSnapshot {
            id: PackageId::new("app-id"),
            manifest_name: "app".to_string(),
            version: semver::Version::new(0, 0, 0),
            ecosystem: Ecosystem::Node,
            path: "packages/app".into(),
            publishable: true,
            dependencies: vec![],
        };
        let core = PackageSnapshot {
            id: PackageId::new("core-id"),
            manifest_name: "core".to_string(),
            version: semver::Version::new(1, 0, 0),
            ecosystem: Ecosystem::Node,
            path: "packages/core".into(),
            publishable: true,
            dependencies: vec![],
        };
        let dependency_package = |id: &str, version: semver::Version| PackageSnapshot {
            id: PackageId::new(id),
            manifest_name: id.to_string(),
            version,
            ecosystem: Ecosystem::Node,
            path: format!("packages/{id}").into(),
            publishable: true,
            dependencies: vec![],
        };
        let workspace_packages = vec![
            package.clone(),
            core,
            dependency_package("workspace", semver::Version::new(1, 0, 0)),
            dependency_package("dev", semver::Version::new(2, 0, 0)),
            dependency_package("peer", semver::Version::new(3, 0, 0)),
            dependency_package("optional", semver::Version::new(4, 0, 0)),
        ];
        let versions = VersionMap::from([
            (PackageId::new("app-id"), semver::Version::new(1, 0, 1)),
            (PackageId::new("core-id"), semver::Version::new(1, 1, 0)),
            (PackageId::new("workspace"), semver::Version::new(1, 1, 0)),
            (PackageId::new("dev"), semver::Version::new(2, 1, 0)),
            (PackageId::new("peer"), semver::Version::new(3, 1, 0)),
            (PackageId::new("optional"), semver::Version::new(4, 1, 0)),
        ]);

        let project_root = camino::Utf8Path::from_path(&root).unwrap();
        let edit = NodejsResolver
            .plan_edits(EcosystemPlanInput {
                project_root,
                workspace_packages: &workspace_packages,
                released_packages: std::slice::from_ref(&package.id),
                versions: &versions,
            })
            .unwrap()
            .remove(0);

        assert_eq!(edit.path.as_str(), "packages/app/package.json");
        assert_eq!(
            edit.expected,
            semifold_core::FileEditExpectation::Existing {
                hash: semifold_core::FileHash::from_bytes(original.as_bytes()),
            }
        );
        assert!(edit.new_content.contains("\"version\": \"1.0.1\""));
        assert!(edit.new_content.contains("\"core\": \"^1.1.0\""));
        assert!(edit.new_content.contains("\"workspace\": \"workspace:*\""));
        assert!(edit.new_content.contains("\"dev\": \"~2.1.0\""));
        assert!(edit.new_content.contains("\"peer\": \"workspace:^3.1.0\""));
        assert!(edit.new_content.contains("\"optional\": \"4.1.0\""));
        assert!(edit.new_content.ends_with('\n'));
        let rendered = serde_json::from_str::<serde_json::Value>(&edit.new_content).unwrap();
        assert_eq!(rendered["custom"]["preserved"], true);
        assert_eq!(
            rendered["custom"]["escaped"],
            serde_json::from_str::<serde_json::Value>(original).unwrap()["custom"]["escaped"]
        );
        assert!(
            edit.new_content.find("\"name\"").unwrap()
                < edit.new_content.find("\"dependencies\"").unwrap()
        );
        assert_eq!(fs::read_to_string(manifest_path).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }
}
