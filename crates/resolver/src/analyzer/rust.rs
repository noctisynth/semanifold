use std::{collections::BTreeMap, path::PathBuf};
use toml::Table;

use crate::analyzer::{InitConfig, Package, ProjectAnalyzer};

pub struct RustAnalyzer {
    pub root: PathBuf,
}

impl ProjectAnalyzer for RustAnalyzer {
    fn analyze(&self) -> anyhow::Result<InitConfig> {
        let package = self.analyze_package(&self.root)?;
        let tags = self.generate_tag()?;
        Ok(InitConfig {
            packages: package,
            tags,
        })
    }
}

impl RustAnalyzer {
    fn analyze_package(&self, root: &PathBuf) -> anyhow::Result<BTreeMap<String, Package>> {
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
                    res_package.insert(name.to_string(), Package { path: root.clone() });
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
