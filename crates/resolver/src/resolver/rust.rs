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

    fn resolve_all(&mut self) -> anyhow::Result<Vec<ResolvedPackage>> {
        unimplemented!()
    }

    fn bump(&mut self, package: &ResolvedPackage, level: BumpLevel) -> anyhow::Result<()> {
        let bumped_version = utils::bump_version(&package.version, level)?.to_string();
        let toml_str = std::fs::read_to_string(package.path.join("Cargo.toml"))?;
        let mut toml_doc = toml_str.parse::<toml_edit::DocumentMut>()?;
        let package_table = toml_doc["package"].as_table_mut().unwrap();
        package_table["version"] = toml_edit::value(bumped_version);
        std::fs::write(package.path.join("Cargo.toml"), toml_doc.to_string())?;
        Ok(())
    }
}
