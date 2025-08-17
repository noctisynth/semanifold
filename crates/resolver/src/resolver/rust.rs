use serde::{Deserialize, Serialize};

use crate::{
    config::{Config, PackageConfig},
    resolver::{ResolvedPackage, Resolver},
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
}

impl Resolver for RustResolver<'_, '_> {
    fn resolve(&self) -> anyhow::Result<ResolvedPackage> {
        let toml_path = self.pkg_config.path.join("Cargo.toml");
        if !toml_path.exists() {
            return Err(anyhow::anyhow!("Failed to find Cargo.toml"));
        }
        let toml_str = std::fs::read_to_string(&toml_path)?;
        let cargo_toml: CargoToml = toml::from_str(&toml_str)?;
        Ok(ResolvedPackage {
            name: cargo_toml.package.name,
            version: cargo_toml.package.version,
            path: self.pkg_config.path.clone(),
        })
    }

    fn bump(&self, _version: &str) -> anyhow::Result<()> {
        todo!()
    }
}
