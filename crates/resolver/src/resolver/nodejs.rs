use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use saphyr::LoadableYamlNode;
use serde::Deserialize;

use crate::{
    config::{PackageConfig, ResolverConfig},
    error::ResolveError,
    resolver::{ResolvedPackage, Resolver, ResolverType},
    utils,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageJson {
    pub name: String,
    pub version: String,
    pub workspaces: Option<Vec<String>>,
    pub dependencies: Option<BTreeMap<String, String>>,
    pub dev_dependencies: Option<BTreeMap<String, String>>,
    pub peer_dependencies: Option<BTreeMap<String, String>>,
    pub private: Option<bool>,
}

pub struct NodejsResolver;

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
            version: package_json.version,
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
                },
            )?;
            return Ok(vec![package]);
        }

        let workspaces = workspaces.unwrap();
        let mut packages = Vec::new();

        for workspace_pattern in workspaces {
            let pattern = format!("{}/{}", root.display(), workspace_pattern);
            let paths = glob::glob(&pattern)
                .map_err(|e| ResolveError::ParseError {
                    path: package_json_path.clone(),
                    reason: e.to_string(),
                })?
                .flatten()
                .collect::<Vec<_>>();

            for path in paths {
                if path.join("package.json").exists() {
                    let rel_path = pathdiff::diff_paths(&path, root).unwrap_or(path.clone());
                    match self.resolve(
                        root,
                        &PackageConfig {
                            path: rel_path,
                            resolver: ResolverType::Nodejs,
                        },
                    ) {
                        Ok(package) => packages.push(package),
                        Err(e) => {
                            log::warn!("Failed to resolve package at {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }

        Ok(packages)
    }

    fn bump(
        &mut self,
        root: &Path,
        package: &ResolvedPackage,
        version: &semver::Version,
        dry_run: bool,
    ) -> Result<(), ResolveError> {
        let bumped_version = version.to_string();
        let package_json_path = root.join(&package.path).join("package.json");
        let package_json_str = std::fs::read_to_string(&package_json_path)?;

        let mut package_json: serde_json::Value =
            serde_json::from_str(&package_json_str).map_err(|e| ResolveError::ParseError {
                path: package_json_path.clone(),
                reason: e.to_string(),
            })?;

        if let Some(obj) = package_json.as_object_mut() {
            obj.insert(
                "version".to_string(),
                serde_json::Value::String(bumped_version.clone()),
            );
        }

        let package_json_content =
            serde_json::to_string_pretty(&package_json).map_err(|e| ResolveError::ParseError {
                path: package_json_path.clone(),
                reason: e.to_string(),
            })?;
        if !dry_run {
            std::fs::write(package_json_path, package_json_content)?;
        } else {
            log::info!(
                "Dry run: Would update {} to version {}",
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
        if dry_run {
            log::warn!(
                "Skip publish {} {} due to dry run",
                package.name,
                format!("v{}", package.version)
            );
            return Ok(());
        } else if package.private {
            log::warn!(
                "Skip publish {} {} due to private flag",
                package.name,
                format!("v{}", package.version)
            );
            return Ok(());
        }

        log::info!("Running prepublish commands for {}", package.name);
        for prepublish in &resolver_config.prepublish {
            let args = prepublish.args.clone().unwrap_or_default();
            log::info!("Running {} {}", prepublish.command, args.join(" "));
            utils::run_command(prepublish, &package.path)?;
        }

        log::info!("Running publish commands for {}", package.name);
        for publish in &resolver_config.publish {
            let args = publish.args.clone().unwrap_or_default();
            log::info!("Running {} {}", publish.command, args.join(" "));
            utils::run_command(publish, &package.path)?;
        }

        Ok(())
    }
}
