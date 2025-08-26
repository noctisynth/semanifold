use serde::{Deserialize, Serialize, Serializer};
use std::collections::BTreeMap;
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
    #[serde(rename = "packages", serialize_with = "inline_btreemap::serialize")]
    packages: BTreeMap<String, Package>,
    tags: BTreeMap<String, String>,
}

pub fn default(root: &PathBuf) -> anyhow::Result<Box<dyn ProjectAnalyzer>> {
    if root.join("Cargo.toml").exists() {
        return Ok(Box::new(rust::RustAnalyzer { root: root.clone() }));
    }
    Err(anyhow::anyhow!("Not found project analyzer"))
}
// 自定义模块实现内联表序列化（针对BTreeMap）
mod inline_btreemap {
    use super::*;
    use serde::ser::SerializeMap;

    // 序列化 BTreeMap 为 TOML 内联表
    pub fn serialize<S>(map: &BTreeMap<String, Package>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map_serializer = serializer.serialize_map(Some(map.len()))?;

        for (k, v) in map {
            map_serializer.serialize_entry(k, v)?;
        }

        map_serializer.end()
    }

    // 反序列化函数（针对BTreeMap）
    pub fn deserialize<'de, D>(deserializer: D) -> Result<BTreeMap<String, Package>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        BTreeMap::deserialize(deserializer)
    }
}
