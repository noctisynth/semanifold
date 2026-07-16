use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use serde::Deserialize;

use crate::{
    config::{PackageConfig, ResolverConfig, VersionMode},
    context,
    error::ResolveError,
    resolver::{ResolvedPackage, Resolver, ResolverType},
    utils,
};

#[derive(Deserialize)]
struct CargoPackage {
    pub name: String,
    pub version: String,
    pub publish: Option<bool>,
}

#[derive(Deserialize)]
struct CargoWorkspace {
    pub members: Vec<String>,
}

#[derive(Deserialize)]
struct CargoToml {
    pub package: Option<CargoPackage>,
    pub workspace: Option<CargoWorkspace>,
    pub dependencies: Option<BTreeMap<String, serde_json::Value>>,
}

pub struct RustResolver;

impl Resolver for RustResolver {
    fn resolve(
        &mut self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<ResolvedPackage, ResolveError> {
        let toml_path = root.join(&pkg_config.path).join("Cargo.toml");
        if !toml_path.exists() {
            return Err(ResolveError::FileOrDirNotFound {
                path: toml_path.clone(),
            });
        }
        let toml_str = std::fs::read_to_string(&toml_path)?;
        let cargo_toml: CargoToml =
            toml_edit::de::from_str(&toml_str).map_err(|e| ResolveError::ParseError {
                path: toml_path.clone(),
                reason: e.to_string(),
            })?;
        let cargo_pkg_config = cargo_toml.package.ok_or(ResolveError::InvalidConfig {
            path: toml_path.clone(),
            reason: "Not found package in Cargo.toml".into(),
        })?;
        let publish = cargo_pkg_config.publish.unwrap_or(true);
        let package = ResolvedPackage {
            name: cargo_pkg_config.name,
            version: semver::Version::parse(&cargo_pkg_config.version)?,
            path: pkg_config.path.clone(),
            private: !publish,
        };
        Ok(package)
    }

    fn resolve_all(&mut self, root: &Path) -> Result<Vec<ResolvedPackage>, ResolveError> {
        let cargo_toml_path = root.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            log::warn!(
                "Cannot resolve package in {}, Cargo.toml not found.",
                root.display()
            );
            return Ok(vec![]);
        }

        let toml_str = std::fs::read_to_string(&cargo_toml_path)?;
        let cargo_toml: CargoToml =
            toml_edit::de::from_str(&toml_str).map_err(|e| ResolveError::ParseError {
                path: cargo_toml_path.clone(),
                reason: e.to_string(),
            })?;

        if cargo_toml.workspace.is_none() {
            if cargo_toml.package.is_none() {
                log::warn!("Failed to resolve package in {}", root.display());
                return Ok(vec![]);
            }
            let package = self.resolve(
                root,
                &PackageConfig {
                    path: ".".into(),
                    resolver: ResolverType::Rust,
                    version_mode: VersionMode::Semantic,
                    assets: vec![],
                },
            )?;
            return Ok(vec![package]);
        }

        let members = cargo_toml.workspace.unwrap().members.iter().try_fold(
            Vec::new(),
            |mut members, member| {
                let pattern = root.join(member).display().to_string();
                let paths = glob::glob(&pattern)
                    .map_err(|e| ResolveError::ParseError {
                        path: cargo_toml_path.clone(),
                        reason: e.to_string(),
                    })?
                    .flatten()
                    .collect::<Vec<_>>();
                members.extend(paths);
                Ok::<_, ResolveError>(members)
            },
        )?;

        log::debug!("members: {members:?}");

        let packages = members
            .into_iter()
            .map(|path| {
                let rel_path = pathdiff::diff_paths(&path, root).unwrap_or(path);
                self.resolve(
                    root,
                    &PackageConfig {
                        path: rel_path.to_path_buf(),
                        resolver: ResolverType::Rust,
                        version_mode: VersionMode::Semantic,
                        assets: vec![],
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(packages)
    }

    fn bump(
        &mut self,
        ctx: &context::Context,
        root: &Path,
        package: &ResolvedPackage,
        version: &semver::Version,
    ) -> Result<(), ResolveError> {
        let bumped_version = version.to_string();
        let cargo_toml_path = root.join(&package.path).join("Cargo.toml");
        let toml_str = std::fs::read_to_string(&cargo_toml_path)?;

        let mut toml_doc =
            toml_str
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| ResolveError::ParseError {
                    path: cargo_toml_path.clone(),
                    reason: e.to_string(),
                })?;
        let package_table = toml_doc["package"]
            .as_table_mut()
            .ok_or(ResolveError::ParseError {
                path: cargo_toml_path.clone(),
                reason: "package table not found".to_string(),
            })?;
        package_table["version"] = toml_edit::value(&bumped_version);

        if let Some(deps_table) = toml_doc["dependencies"].as_table_mut() {
            for (name, bumped_version) in ctx.version_bumps.borrow().iter() {
                if let Some(dep) = deps_table.get_mut(name)
                    && dep.get("version").is_some_and(|version| version.is_str())
                {
                    dep["version"] = toml_edit::value(bumped_version.to_string());
                }
            }
        }

        let toml_content = toml_doc.to_string();
        if !ctx.dry_run {
            std::fs::write(cargo_toml_path, toml_content)?;
        } else {
            log::warn!(
                "Skip bump for {} to version {} due to dry run",
                package.name,
                bumped_version
            );
        }
        Ok(())
    }

    fn sort_packages(
        &mut self,
        root: &Path,
        packages: &mut Vec<(String, PackageConfig)>,
    ) -> Result<(), ResolveError> {
        let cached_packages = packages
            .iter()
            .filter(|(_, cfg)| cfg.resolver == ResolverType::Rust)
            .try_fold(HashMap::new(), |mut acc, (name, cfg)| {
                let cargo_toml: CargoToml = toml_edit::de::from_str(&std::fs::read_to_string(
                    root.join(&cfg.path).join("Cargo.toml"),
                )?)
                .map_err(|e| ResolveError::ParseError {
                    path: cfg.path.join("Cargo.toml"),
                    reason: e.to_string(),
                })?;
                acc.insert(name.clone(), cargo_toml);
                Ok::<_, ResolveError>(acc)
            })?;

        packages.sort_by(
            |(a, a_cfg), (b, b_cfg)| match (a_cfg.resolver, b_cfg.resolver) {
                (ResolverType::Rust, ResolverType::Rust) => {
                    let a_deps = cached_packages
                        .get(a)
                        .unwrap()
                        .dependencies
                        .as_ref()
                        .unwrap();
                    let b_deps = cached_packages
                        .get(b)
                        .unwrap()
                        .dependencies
                        .as_ref()
                        .unwrap();
                    if a_deps.contains_key(b) {
                        std::cmp::Ordering::Greater
                    } else if b_deps.contains_key(a) {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                }
                _ => std::cmp::Ordering::Equal,
            },
        );

        Ok(())
    }

    fn publish(
        &mut self,
        package: &ResolvedPackage,
        resolver_config: &ResolverConfig,
        dry_run: bool,
    ) -> Result<(), ResolveError> {
        if package.private {
            log::warn!(
                "Skip publish {} {} due to private flag",
                package.name,
                format_args!("v{}", package.version)
            );
            return Ok(());
        }

        log::info!("Running prepublish commands for {}", package.name);
        for prepublish in &resolver_config.prepublish {
            let args = prepublish.args.clone().unwrap_or_default();
            if dry_run && !prepublish.dry_run.unwrap_or(false) {
                log::warn!(
                    "Skip prepublish command {} {} due to dry run",
                    prepublish.command,
                    args.join(" ")
                );
                continue;
            }
            log::info!("Running {} {}", prepublish.command, args.join(" "));
            utils::run_command(prepublish, &package.path)?;
        }

        log::info!("Running publish commands for {}", package.name);
        for publish in &resolver_config.publish {
            let args = publish.args.clone().unwrap_or_default();
            if dry_run && !publish.dry_run.unwrap_or(false) {
                log::warn!(
                    "Skip publish command {} {} due to dry run",
                    publish.command,
                    args.join(" ")
                );
                continue;
            }
            log::info!("Running {} {}", publish.command, args.join(" "));
            utils::run_command(publish, &package.path)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        config::{PackageConfig, VersionMode},
        context::Context,
        resolver::{ResolvedPackage, Resolver, ResolverType},
    };

    use super::RustResolver;

    fn temp_dir(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "semifold-rust-resolver-{test_name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn package_config(path: impl Into<PathBuf>) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Rust,
            version_mode: VersionMode::Semantic,
            assets: vec![],
        }
    }

    fn write_package(
        root: &Path,
        path: &str,
        name: &str,
        version: &str,
        publish: Option<bool>,
        dependencies: Option<&str>,
    ) {
        let package_root = root.join(path);
        fs::create_dir_all(&package_root).unwrap();
        let publish = publish
            .map(|value| format!("publish = {value}\n"))
            .unwrap_or_default();
        let dependencies = dependencies
            .map(|value| format!("\n[dependencies]\n{value}\n"))
            .unwrap_or_default();
        fs::write(
            package_root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"{version}\"\n{publish}{dependencies}"
            ),
        )
        .unwrap();
    }

    #[test]
    fn resolves_a_single_package() {
        let root = temp_dir("single-package");
        write_package(&root, ".", "single", "1.2.3", None, None);

        let package = RustResolver.resolve(&root, &package_config(".")).unwrap();

        assert_eq!(package.name, "single");
        assert_eq!(package.version, semver::Version::parse("1.2.3").unwrap());
        assert!(!package.private);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_workspace_members_and_private_packages() {
        let root = temp_dir("workspace");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        write_package(&root, "crates/core", "core", "1.0.0", None, None);
        write_package(
            &root,
            "crates/internal",
            "internal",
            "1.0.0",
            Some(false),
            None,
        );

        let mut packages = RustResolver.resolve_all(&root).unwrap();
        packages.sort_by(|left, right| left.name.cmp(&right.name));

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "core");
        assert_eq!(packages[0].path, PathBuf::from("crates/core"));
        assert!(!packages[0].private);
        assert_eq!(packages[1].name, "internal");
        assert_eq!(packages[1].path, PathBuf::from("crates/internal"));
        assert!(packages[1].private);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bumps_a_package_and_its_inline_internal_dependency() {
        let root = temp_dir("bump");
        write_package(&root, "crates/core", "core", "1.0.0", None, None);
        write_package(
            &root,
            "crates/app",
            "app",
            "1.0.0",
            None,
            Some("core = { version = \"1.0.0\", path = \"../core\" }"),
        );

        let ctx = Context::default();
        ctx.version_bumps
            .borrow_mut()
            .insert("core".to_string(), semver::Version::parse("1.1.0").unwrap());
        let app = ResolvedPackage {
            name: "app".to_string(),
            version: semver::Version::parse("1.0.0").unwrap(),
            path: PathBuf::from("crates/app"),
            private: false,
        };

        RustResolver
            .bump(&ctx, &root, &app, &semver::Version::parse("1.0.1").unwrap())
            .unwrap();

        let manifest = fs::read_to_string(root.join("crates/app/Cargo.toml")).unwrap();
        assert!(manifest.contains("version = \"1.0.1\""));
        assert!(manifest.contains("core = { version = \"1.1.0\", path = \"../core\" }"));
        fs::remove_dir_all(root).unwrap();
    }
}
