use serde::Serializer;
use serde::ser::SerializeMap;
use std::collections::BTreeMap;

use crate::resolver::ConfigPackage;

// 序列化 BTreeMap 为 TOML 内联表
pub fn serialize<S>(map: &BTreeMap<String, ConfigPackage>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map_serializer = serializer.serialize_map(Some(map.len()))?;

    for (k, v) in map {
        map_serializer.serialize_entry(k, v)?;
    }

    map_serializer.end()
}
