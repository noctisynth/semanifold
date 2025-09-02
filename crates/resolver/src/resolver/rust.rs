use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    changeset::BumpLevel,
    config::{Config, PackageConfig},
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

pub struct RustResolver<'c> {
    pub config: &'c Config,
}

impl Resolver for RustResolver<'_> {
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

    fn resolve_all(&mut self, _root: &Path) -> Result<Vec<ResolvedPackage>, ResolveError> {
        todo!()
        // let mut res_package = BTreeMap::new();

        // let config = root
        //     .read_dir()?
        //     .filter_map(Result::ok)
        //     .find(|entry| {
        //         entry.path().is_file()
        //             && entry
        //                 .file_name()
        //                 .to_str()
        //                 .map(|name| name.eq_ignore_ascii_case("Cargo.toml"))
        //                 .unwrap_or(false)
        //     })
        //     .map(|entry| entry.path());

        // let Some(config_path) = config else {
        //     log::warn!("Not found Cargo.toml in {}", root.display());
        //     return Ok(res_package);
        // };

        // let doc = std::fs::read_to_string(config_path)?.parse::<Table>()?;

        // if let Some(package) = doc.get("package").and_then(|value| value.as_table()) {
        //     match package["name"].as_str() {
        //         Some(name) => {
        //             res_package.insert(name.to_string(), ConfigPackage { path: root.clone() });
        //         }
        //         None => {
        //             log::warn!("Not found package name in {}", root.display());
        //             return Ok(res_package);
        //         }
        //     }
        // }

        // let Some(workspace) = doc.get("workspace").and_then(|v| v.as_table()) else {
        //     return Ok(res_package);
        // };
        // let Some(members) = workspace.get("members").and_then(|v| v.as_array()) else {
        //     return Ok(res_package);
        // };

        // members
        //     .iter()
        //     .filter_map(|map| map.as_str())
        //     .flat_map(|pattern| glob::glob(&root.join(pattern).to_string_lossy()).ok())
        //     .flatten()
        //     .filter_map(Result::ok)
        //     .for_each(|path| {
        //         log::info!("Found package in {}", path.display());
        //         if let Ok(package) = self.analyze_package(&path) {
        //             res_package.extend(package);
        //         }
        //     });

        // return Ok(res_package);
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
