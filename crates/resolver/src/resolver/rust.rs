use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use toml::Table;

use crate::{
    changeset::BumpLevel,
    config::{Config, PackageConfig},
    error::ResolveError,
    resolver::{ChangesConfig, ConfigPackage, ResolvedPackage, Resolver},
    utils,
};

#[derive(Serialize, Deserialize)]
struct CargoPackage {
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize)]
struct CargoToml {
    pub package: CargoPackage,
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
        let package = ResolvedPackage {
            name: cargo_toml.package.name,
            version: cargo_toml.package.version,
            path: pkg_config.path.clone(),
        };
        Ok(package)
    }

    fn resolve_all(&mut self) -> Result<Vec<ResolvedPackage>, ResolveError> {
        unimplemented!()
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

    fn analyze_project(&mut self, root: &PathBuf) -> anyhow::Result<ChangesConfig> {
        let package = self.analyze_package(root)?;
        let tags = self.generate_tag()?;
        Ok(ChangesConfig {
            packages: package,
            tags,
        })
    }
}

impl RustResolver<'_> {
    fn analyze_package(&self, root: &PathBuf) -> anyhow::Result<BTreeMap<String, ConfigPackage>> {
        let mut res_package = BTreeMap::new();

        let config = root
            .read_dir()?
            .filter_map(Result::ok)
            .find(|entry| {
                entry.path().is_file()
                    && entry
                        .file_name()
                        .to_str()
                        .map(|name| name.eq_ignore_ascii_case("Cargo.toml"))
                        .unwrap_or(false)
            })
            .map(|entry| entry.path());

        let Some(config_path) = config else {
            log::warn!("Not found Cargo.toml in {}", root.display());
            return Ok(res_package);
        };

        let doc = std::fs::read_to_string(config_path)?.parse::<Table>()?;

        if let Some(package) = doc.get("package").and_then(|value| value.as_table()) {
            match package["name"].as_str() {
                Some(name) => {
                    res_package.insert(name.to_string(), ConfigPackage { path: root.clone() });
                }
                None => {
                    log::warn!("Not found package name in {}", root.display());
                    return Ok(res_package);
                }
            }
        }

        let Some(workspace) = doc.get("workspace").and_then(|v| v.as_table()) else {
            return Ok(res_package);
        };
        let Some(members) = workspace.get("members").and_then(|v| v.as_array()) else {
            return Ok(res_package);
        };

        members
            .iter()
            .filter_map(|map| map.as_str())
            .flat_map(|pattern| glob::glob(&root.join(pattern).to_string_lossy()).ok())
            .flatten()
            .filter_map(Result::ok)
            .for_each(|path| {
                log::info!("Found package in {}", path.display());
                if let Ok(package) = self.analyze_package(&path) {
                    res_package.extend(package);
                }
            });

        return Ok(res_package);
    }

    fn generate_tag(&self) -> anyhow::Result<BTreeMap<String, String>> {
        // 获取用户输入
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        // 根据输入决定是否初始化map
        if input == "y" || input == "1" {
            Ok(BTreeMap::from_iter([
                ("chore".to_string(), "Chore".to_string()),
                ("feat".to_string(), "New Feature".to_string()),
                ("fix".to_string(), "Bug Fix".to_string()),
                ("perf".to_string(), "Performance Improvement".to_string()),
                ("refactor".to_string(), "Refactor".to_string()),
            ]))
        } else {
            Ok(BTreeMap::new())
        }
    }
}
