use serde::{Deserialize, Serialize};

use crate::{
    changeset::BumpLevel,
    config::{Config, PackageConfig},
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

pub struct RustResolver<'p, 'c>
where
    'p: 'c,
{
    pub pkg_config: &'p PackageConfig,
    pub config: &'c Config,
    pub package: Option<ResolvedPackage>,
}

impl Resolver for RustResolver<'_, '_> {
    fn resolve<'r>(&'r mut self) -> anyhow::Result<&'r ResolvedPackage> {
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
        let toml_str = std::fs::read_to_string(&self.pkg_config.path.join("Cargo.toml"))?;
        let mut toml_doc = toml_str.parse::<toml_edit::DocumentMut>()?;
        let package = toml_doc["package"].as_table_mut().unwrap();
        package["version"] = toml_edit::value(bumped_version);
        std::fs::write(
            &self.pkg_config.path.join("Cargo.toml"),
            toml_doc.to_string(),
        )?;
        Ok(())
    }
}
