use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod rust;

// 项目分析器
pub trait ProjectAnalyzer {
    fn analyze(&self) -> anyhow::Result<InitConfig>;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    pub path: PathBuf,
}

//
#[derive(Serialize, Deserialize)]
pub struct InitConfig {
    #[serde(rename = "packages")]
    packages: HashMap<String, Package>,
    tags: std::collections::HashMap<String, String>,
}

pub fn default(root: &PathBuf) -> anyhow::Result<Box<dyn ProjectAnalyzer>> {
    if root.join("Cargo.toml").exists() {
        return Ok(Box::new(rust::RustAnalyzer { root: root.clone() }));
    }
    Err(anyhow::anyhow!("Not found project analyzer"))
}
