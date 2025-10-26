use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

use crate::{
    config::{PackageConfig, ResolverConfig},
    error::ResolveError,
    resolver::{ResolvedPackage, Resolver, ResolverType},
};

#[derive(Serialize, Deserialize)]
struct CargoPackage {
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize)]
struct CargoWorkspace {
    pub members: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct CargoToml {
    pub package: Option<CargoPackage>,
    pub workspace: Option<CargoWorkspace>,
    pub dependencies: Option<BTreeMap<String, serde_json::Value>>,
}

pub struct RustResolver;

impl Resolver for RustResolver {
    fn resolve(
        &mut self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<ResolvedPackage, ResolveError> {
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
        let package = ResolvedPackage {
            name: cargo_pkg_config.name,
            version: cargo_pkg_config.version,
            path: pkg_config.path.clone(),
        };
        Ok(package)
    }

    fn resolve_all(&mut self, root: &Path) -> Result<Vec<ResolvedPackage>, ResolveError> {
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
            let package = self.resolve(
                root,
                &PackageConfig {
                    path: root.to_path_buf(),
                    resolver: ResolverType::Rust,
                },
            )?;
            return Ok(vec![package]);
        }

        let members = cargo_toml.workspace.unwrap().members.iter().try_fold(
            Vec::new(),
            |mut members, member| {
                let pattern = root.join(member).to_string_lossy().into_owned();
                let paths = glob::glob(&pattern)
                    .map_err(|e| ResolveError::ParseError {
                        path: cargo_toml_path.clone(),
                        reason: e.to_string(),
                    })?
                    .flatten()
                    .collect::<Vec<_>>();
                members.extend(paths);
                Ok::<_, ResolveError>(members)
            },
        )?;

        log::debug!("members: {members:?}");

        let packages = members
            .into_iter()
            .map(|path| {
                let rel_path = pathdiff::diff_paths(&path, root).unwrap_or(path);
                self.resolve(
                    root,
                    &PackageConfig {
                        path: rel_path.to_path_buf(),
                        resolver: ResolverType::Rust,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

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
        let cargo_toml_path = root.join(&package.path).join("Cargo.toml");
        let toml_str = std::fs::read_to_string(&cargo_toml_path)?;

        let mut toml_doc =
            toml_str
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| ResolveError::ParseError {
                    path: cargo_toml_path.clone(),
                    reason: e.to_string(),
                })?;
        let package_table =
            toml_doc["package"]
                .as_table_mut()
                .ok_or_else(|| ResolveError::ParseError {
                    path: cargo_toml_path.clone(),
                    reason: "package table not found".to_string(),
                })?;
        package_table["version"] = toml_edit::value(&bumped_version);

        let toml_content = toml_doc.to_string();
        if !dry_run {
            std::fs::write(cargo_toml_path, toml_content)?;
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
        packages: &mut Vec<(String, PackageConfig)>,
    ) -> Result<(), ResolveError> {
        let cached_packages = packages
            .iter()
            .filter(|(_, cfg)| cfg.resolver == ResolverType::Rust)
            .try_fold(HashMap::new(), |mut acc, (name, cfg)| {
                let cargo_toml: CargoToml =
                    toml_edit::de::from_str(&std::fs::read_to_string(cfg.path.join("Cargo.toml"))?)
                        .map_err(|e| ResolveError::ParseError {
                            path: cfg.path.join("Cargo.toml"),
                            reason: e.to_string(),
                        })?;
                acc.insert(name.clone(), cargo_toml);
                Ok::<_, ResolveError>(acc)
            })?;

        packages.sort_by(
            |(a, a_cfg), (b, b_cfg)| match (a_cfg.resolver, b_cfg.resolver) {
                (ResolverType::Rust, ResolverType::Rust) => {
                    let a_deps = cached_packages
                        .get(a)
                        .unwrap()
                        .dependencies
                        .as_ref()
                        .unwrap();
                    let b_deps = cached_packages
                        .get(b)
                        .unwrap()
                        .dependencies
                        .as_ref()
                        .unwrap();
                    if a_deps.contains_key(b) {
                        std::cmp::Ordering::Greater
                    } else if b_deps.contains_key(a) {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                }
                #[allow(unreachable_patterns)]
                _ => std::cmp::Ordering::Equal,
            },
        );

        Ok(())
    }

    fn publish(
        &mut self,
        package: &ResolvedPackage,
        resolver_config: &ResolverConfig,
        dry_run: bool,
    ) -> Result<(), ResolveError> {
        if dry_run {
            log::info!(
                "Dry run: Would publish {} to version {}",
                package.name,
                package.version
            );
            return Ok(());
        }

        log::info!("Running prepublish commands for {}", package.name);
        for prepublish in &resolver_config.prepublish {
            let args = prepublish.args.clone().unwrap_or_default();
            log::info!("Running {} {}", prepublish.command, args.join(" "));
            let output = Command::new(&prepublish.command)
                .args(&args)
                .current_dir(&package.path)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .output()?;
            if !output.status.success() {
                return Err(ResolveError::PublishError {
                    package: package.name.clone(),
                    reason: format!(
                        "Prepublish command {} failed with status {:?} (code: {:?})",
                        prepublish.command,
                        output.status,
                        output.status.code()
                    ),
                });
            }
        }

        log::info!("Running publish commands for {}", package.name);
        for publish in &resolver_config.publish {
            let args = publish.args.clone().unwrap_or_default();
            log::info!("Running {} {}", publish.command, args.join(" "));
            Command::new(&publish.command)
                .args(&args)
                .current_dir(&package.path)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .output()?;
        }

        Ok(())
    }
}
