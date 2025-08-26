use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use toml::Table;

use crate::{
    changeset::BumpLevel,
    config::{Config, PackageConfig},
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

pub struct RustResolver<'p, 'c>
where
    'p: 'c,
{
    pub pkg_config: &'p PackageConfig,
    pub config: &'c Config,
    pub package: Option<ResolvedPackage>,
}

impl Resolver for RustResolver<'_, '_> {
    fn resolve(&mut self) -> anyhow::Result<&ResolvedPackage> {
        if self.package.is_some() {
            return Ok(self.package.as_ref().unwrap());
        }

        let toml_path = self.pkg_config.path.join("Cargo.toml");
        if !toml_path.exists() {
            return Err(anyhow::anyhow!("Failed to find Cargo.toml"));
        }
        let toml_str = std::fs::read_to_string(&toml_path)?;
        let cargo_toml: CargoToml = toml::from_str(&toml_str)?;
        let package = ResolvedPackage {
            name: cargo_toml.package.name,
            version: cargo_toml.package.version,
            path: self.pkg_config.path.clone(),
        };
        self.package = Some(package);
        Ok(self.package.as_ref().unwrap())
    }

    fn bump(&mut self, level: BumpLevel) -> anyhow::Result<()> {
        let package = self.resolve()?;

        let bumped_version = utils::bump_version(&package.version, level)?.to_string();
        let toml_str = std::fs::read_to_string(self.pkg_config.path.join("Cargo.toml"))?;
        let mut toml_doc = toml_str.parse::<toml_edit::DocumentMut>()?;
        let package = toml_doc["package"].as_table_mut().unwrap();
        package["version"] = toml_edit::value(bumped_version);
        std::fs::write(
            self.pkg_config.path.join("Cargo.toml"),
            toml_doc.to_string(),
        )?;
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

impl RustResolver<'_, '_> {
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
