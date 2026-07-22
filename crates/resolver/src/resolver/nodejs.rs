use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use saphyr::LoadableYamlNode;
use semifold_core::{
    DependencyKind, EditSource, FileEdit, FileHash, PackageId, PackageSnapshot, VersionMap,
};
use serde::Deserialize;

use crate::{
    config::{PackageConfig, ReleaseChannel, ResolverConfig},
    context,
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
    /// Plans a package.json replacement from immutable package and version snapshots.
    pub fn plan_file_edit(
        root: &Path,
        package: &PackageSnapshot,
        versions: &VersionMap,
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
                let Some(version) = versions.get(&PackageId::new(name.as_str())) else {
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
            expected_hash: FileHash::from_bytes(original.as_bytes()),
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
        let workspaces = pnpm_packages.or(package_json.workspaces);
        if workspaces.is_none() {
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
                },
            )?;
            return Ok(vec![package]);
        }

        let workspaces = workspaces.unwrap();
        let mut packages = Vec::new();

        let root_package = self.resolve(
            root,
            &PackageConfig {
                path: ".".into(),
                resolver: ResolverType::Nodejs,
                channel: ReleaseChannel::Stable,
                assets: vec![],
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

    fn bump(
        &mut self,
        ctx: &context::Context,
        root: &Path,
        package: &ResolvedPackage,
        version: &semver::Version,
    ) -> Result<(), ResolveError> {
        let bumped_version = version.to_string();
        let package_json_path = root.join(&package.path).join("package.json");
        let package_json_str = std::fs::read_to_string(&package_json_path)?;

        let mut package_json: serde_json::Value =
            serde_json::from_str(&package_json_str).map_err(|e| ResolveError::ParseError {
                path: package_json_path.clone(),
                reason: e.to_string(),
            })?;
        let object = package_json
            .as_object_mut()
            .ok_or(ResolveError::ParseError {
                path: package_json_path.clone(),
                reason: "package.json root must be an object".to_string(),
            })?;
        object.insert(
            "version".to_string(),
            serde_json::Value::String(bumped_version.clone()),
        );
        let mut package_json_content =
            serde_json::to_string_pretty(&package_json).map_err(|error| {
                ResolveError::ParseError {
                    path: package_json_path.clone(),
                    reason: error.to_string(),
                }
            })?;
        package_json_content.push('\n');
        if !ctx.dry_run {
            std::fs::write(package_json_path, package_json_content)?;
        } else {
            log::warn!(
                "Skip bump for {} to version {} due to dry run",
                package.name,
                bumped_version
            );
        }
        Ok(())
    }

    fn sort_packages(
        &mut self,
        root: &Path,
        packages: &mut Vec<(String, PackageConfig)>,
    ) -> Result<(), ResolveError> {
        let cached_packages = packages
            .iter()
            .filter(|(_, cfg)| cfg.resolver == ResolverType::Nodejs)
            .try_fold(HashMap::new(), |mut acc, (name, cfg)| {
                let package_json: PackageJson = serde_json::from_str(&std::fs::read_to_string(
                    root.join(&cfg.path).join("package.json"),
                )?)
                .map_err(|e| ResolveError::ParseError {
                    path: cfg.path.join("package.json"),
                    reason: e.to_string(),
                })?;
                acc.insert(name.clone(), package_json);
                Ok::<_, ResolveError>(acc)
            })?;

        packages.sort_by(|(a, a_cfg), (b, b_cfg)| {
            if a_cfg.resolver == ResolverType::Nodejs && b_cfg.resolver == ResolverType::Nodejs {
                let a_pkg = cached_packages.get(a).unwrap();
                let b_pkg = cached_packages.get(b).unwrap();

                // 检查依赖关系
                let has_dep = |pkg: &PackageJson, dep_name: &str| -> bool {
                    pkg.dependencies
                        .as_ref()
                        .is_some_and(|deps| deps.contains_key(dep_name))
                        || pkg
                            .dev_dependencies
                            .as_ref()
                            .is_some_and(|deps| deps.contains_key(dep_name))
                        || pkg
                            .peer_dependencies
                            .as_ref()
                            .is_some_and(|deps| deps.contains_key(dep_name))
                };

                if has_dep(a_pkg, b) {
                    std::cmp::Ordering::Greater
                } else if has_dep(b_pkg, a) {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            } else {
                std::cmp::Ordering::Equal
            }
        });

        Ok(())
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
        config::{PackageConfig, ReleaseChannel},
        context::Context,
        resolver::{ResolvedPackage, Resolver, ResolverType},
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
    fn bumps_package_version_without_removing_other_json_fields() {
        let root = temp_dir("bump");
        write_package(
            &root,
            "packages/app",
            "app",
            "1.0.0",
            false,
            ",\n  \"dependencies\": { \"core\": \"^1.0.0\" },\n  \"custom\": { \"preserved\": true }",
        );
        let app = ResolvedPackage {
            name: "app".to_string(),
            version: semver::Version::parse("1.0.0").unwrap(),
            path: PathBuf::from("packages/app"),
            private: false,
        };

        NodejsResolver
            .bump(
                &Context::default(),
                &root,
                &app,
                &semver::Version::parse("1.0.1").unwrap(),
            )
            .unwrap();

        let package_json = serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(root.join("packages/app/package.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(package_json["version"], "1.0.1");
        assert_eq!(package_json["dependencies"]["core"], "^1.0.0");
        assert_eq!(package_json["custom"]["preserved"], true);
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
            id: PackageId::new("app"),
            manifest_name: "app".to_string(),
            version: semver::Version::new(0, 0, 0),
            ecosystem: Ecosystem::Node,
            path: "packages/app".into(),
            publishable: true,
            dependencies: vec![],
        };
        let versions = VersionMap::from([
            (PackageId::new("app"), semver::Version::new(1, 0, 1)),
            (PackageId::new("core"), semver::Version::new(1, 1, 0)),
            (PackageId::new("workspace"), semver::Version::new(1, 1, 0)),
            (PackageId::new("dev"), semver::Version::new(2, 1, 0)),
            (PackageId::new("peer"), semver::Version::new(3, 1, 0)),
            (PackageId::new("optional"), semver::Version::new(4, 1, 0)),
        ]);

        let edit = NodejsResolver::plan_file_edit(&root, &package, &versions).unwrap();

        assert_eq!(edit.path.as_str(), "packages/app/package.json");
        assert_eq!(
            edit.expected_hash,
            semifold_core::FileHash::from_bytes(original.as_bytes())
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
