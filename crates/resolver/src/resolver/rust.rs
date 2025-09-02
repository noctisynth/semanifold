use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    changeset::BumpLevel,
    config::PackageConfig,
    error::ResolveError,
    resolver::{ResolvedPackage, Resolver},
    utils,
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
}

pub struct RustResolver;

impl Resolver for RustResolver {
    fn resolve(&mut self, pkg_config: &PackageConfig) -> Result<ResolvedPackage, ResolveError> {
        let toml_path = pkg_config.path.join("Cargo.toml");
        if !toml_path.exists() {
            return Err(ResolveError::FileOrDirNotFound {
                path: toml_path.clone(),
            });
        }
        let toml_str = std::fs::read_to_string(&toml_path)?;
        let cargo_toml: CargoToml =
            toml::from_str(&toml_str).map_err(|e| ResolveError::ParseError {
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
            toml::from_str(&toml_str).map_err(|e| ResolveError::ParseError {
                path: cargo_toml_path.clone(),
                reason: e.to_string(),
            })?;

        if cargo_toml.workspace.is_none() {
            if cargo_toml.package.is_none() {
                log::warn!("Failed to resolve package in {}", root.display());
                return Ok(vec![]);
            }
            let package = self.resolve(&PackageConfig {
                path: root.to_path_buf(),
            })?;
            return Ok(vec![package]);
        }

        let members = cargo_toml
            .workspace
            .unwrap()
            .members
            .iter()
            .map(|member| glob::glob(&root.join(member).to_string_lossy()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ResolveError::ParseError {
                path: cargo_toml_path.clone(),
                reason: e.to_string(),
            })?
            .into_iter()
            .flatten()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ResolveError::ParseError {
                path: cargo_toml_path.clone(),
                reason: e.to_string(),
            })?;

        log::debug!("members: {:?}", members);

        let packages = members
            .into_iter()
            .map(|path| {
                self.resolve(&PackageConfig {
                    path: PathBuf::from(path.to_string_lossy().replace(
                        &[root.to_string_lossy().to_string(), "".into()].join("/"),
                        "",
                    )),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(packages)
    }

    fn bump(&mut self, package: &ResolvedPackage, level: BumpLevel) -> Result<(), ResolveError> {
        let bumped_version = utils::bump_version(&package.version, level)?.to_string();
        let toml_str = std::fs::read_to_string(package.path.join("Cargo.toml"))?;
        let mut toml_doc =
            toml_str
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| ResolveError::ParseError {
                    path: package.path.join("Cargo.toml"),
                    reason: e.to_string(),
                })?;
        let package_table =
            toml_doc["package"]
                .as_table_mut()
                .ok_or_else(|| ResolveError::ParseError {
                    path: package.path.join("Cargo.toml"),
                    reason: "package table not found".to_string(),
                })?;
        package_table["version"] = toml_edit::value(bumped_version);
        std::fs::write(package.path.join("Cargo.toml"), toml_doc.to_string())?;
        Ok(())
    }
}
