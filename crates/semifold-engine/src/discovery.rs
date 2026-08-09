use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

use semifold_core::{DiscoveredPackage, EcosystemId, PackageId};
use semifold_resolver::{
    adapter::{AdapterError, EcosystemAdapter},
    config::{PackageConfig, ReleaseChannel},
    resolver::{
        ResolverType, cpp::CppResolver, nodejs::NodejsResolver, python::PythonResolver,
        rust::RustResolver,
    },
};

use crate::package_path::{PackagePathError, normalize_package_path};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageDiscovery {
    pub resolvers: Vec<ResolverType>,
    pub packages: Vec<DiscoveredPackage>,
}

impl PackageDiscovery {
    pub fn default_package_configs(
        &self,
    ) -> Result<BTreeMap<String, PackageConfig>, PackageDiscoveryError> {
        let mut configs = BTreeMap::new();
        for package in &self.packages {
            let resolver =
                ResolverRegistry::resolver_type(&package.ecosystem).ok_or_else(|| {
                    PackageDiscoveryError::ResolverUnavailable {
                        ecosystem: package.ecosystem.clone(),
                    }
                })?;
            let config = PackageConfig {
                path: PathBuf::from(package.path.as_str()),
                resolver,
                channel: ReleaseChannel::Stable,
                channel_bump: None,
                assets: vec![],
                github_release: None,
                depends_on: vec![],
            };
            if configs.insert(package.id.to_string(), config).is_some() {
                return Err(PackageDiscoveryError::DuplicatePackageId {
                    package: package.id.clone(),
                });
            }
        }
        Ok(configs)
    }
}

#[derive(Default)]
pub struct ResolverRegistry;

impl ResolverRegistry {
    pub fn normalize_selection(resolvers: &[ResolverType]) -> Vec<ResolverType> {
        let mut resolvers = resolvers.to_vec();
        resolvers.sort();
        resolvers.dedup();
        resolvers
    }

    pub const fn ecosystem(resolver: ResolverType) -> EcosystemId {
        match resolver {
            ResolverType::Rust => EcosystemId::RUST,
            ResolverType::Nodejs => EcosystemId::NODE,
            ResolverType::Python => EcosystemId::PYTHON,
            ResolverType::Cpp => EcosystemId::CPP,
        }
    }

    fn resolver_type(ecosystem: &EcosystemId) -> Option<ResolverType> {
        match ecosystem.as_str() {
            "rust" => Some(ResolverType::Rust),
            "nodejs" => Some(ResolverType::Nodejs),
            "python" => Some(ResolverType::Python),
            "cpp" => Some(ResolverType::Cpp),
            _ => None,
        }
    }

    pub fn create_adapter(&self, resolver: ResolverType) -> Box<dyn EcosystemAdapter> {
        match resolver {
            ResolverType::Rust => Box::new(RustResolver),
            ResolverType::Nodejs => Box::new(NodejsResolver),
            ResolverType::Python => Box::new(PythonResolver),
            ResolverType::Cpp => Box::new(CppResolver),
        }
    }
}

#[derive(Default)]
pub struct PackageDiscoveryService {
    registry: ResolverRegistry,
}

impl PackageDiscoveryService {
    pub fn discover(
        &self,
        project_root: &Path,
        resolvers: &[ResolverType],
    ) -> Result<PackageDiscovery, PackageDiscoveryError> {
        let resolvers = ResolverRegistry::normalize_selection(resolvers);
        let mut packages = Vec::new();
        for resolver_type in &resolvers {
            let root = camino::Utf8Path::from_path(project_root).ok_or_else(|| {
                PackageDiscoveryError::InvalidProjectRoot {
                    path: project_root.to_path_buf(),
                }
            })?;
            let mut discovered = self
                .registry
                .create_adapter(*resolver_type)
                .discover(root)
                .map_err(|source| PackageDiscoveryError::Adapter {
                    resolver: *resolver_type,
                    source,
                })?
                .into_iter()
                .map(|package| {
                    let path = normalize_package_path(project_root, package.path.as_std_path())
                        .map_err(|source| PackageDiscoveryError::PackagePath {
                            resolver: *resolver_type,
                            package: package.manifest_name.clone(),
                            source,
                        })?;
                    Ok(DiscoveredPackage {
                        id: package.id,
                        ecosystem: package.ecosystem,
                        path,
                    })
                })
                .collect::<Result<Vec<_>, PackageDiscoveryError>>()?;
            packages.append(&mut discovered);
        }
        packages.sort();

        Ok(PackageDiscovery {
            resolvers,
            packages,
        })
    }
}

#[derive(Debug)]
pub enum PackageDiscoveryError {
    Adapter {
        resolver: ResolverType,
        source: AdapterError,
    },
    InvalidProjectRoot {
        path: PathBuf,
    },
    PackagePath {
        resolver: ResolverType,
        package: String,
        source: PackagePathError,
    },
    DuplicatePackageId {
        package: PackageId,
    },
    ResolverUnavailable {
        ecosystem: EcosystemId,
    },
}

impl fmt::Display for PackageDiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter { resolver, source } => {
                write!(formatter, "{resolver} package discovery failed: {source}")
            }
            Self::InvalidProjectRoot { path } => {
                write!(
                    formatter,
                    "package discovery project root is not valid UTF-8: {}",
                    path.display()
                )
            }
            Self::PackagePath {
                resolver,
                package,
                source,
            } => write!(
                formatter,
                "{resolver} package discovery returned an invalid path for {package}: {source}"
            ),
            Self::DuplicatePackageId { package } => {
                write!(
                    formatter,
                    "package discovery returned duplicate package id: {package}"
                )
            }
            Self::ResolverUnavailable { ecosystem } => write!(
                formatter,
                "no configured resolver is available for dynamic ecosystem {ecosystem}"
            ),
        }
    }
}

impl Error for PackageDiscoveryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Adapter { source, .. } => Some(source),
            Self::PackagePath { source, .. } => Some(source),
            Self::InvalidProjectRoot { .. }
            | Self::DuplicatePackageId { .. }
            | Self::ResolverUnavailable { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use camino::Utf8PathBuf;

    use super::*;

    struct TemporaryRoot(PathBuf);

    impl TemporaryRoot {
        fn new(name: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "semifold-discovery-{name}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TemporaryRoot {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn discovers_selected_ecosystems_with_stable_order_and_normalized_paths() {
        let root = TemporaryRoot::new("mixed");
        fs::write(
            root.0.join("Cargo.toml"),
            "[package]\nname = \"rust-app\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        fs::write(
            root.0.join("package.json"),
            r#"{"name":"node-app","version":"1.0.0"}"#,
        )
        .unwrap();

        let discovery = PackageDiscoveryService::default()
            .discover(
                &root.0,
                &[
                    ResolverType::Nodejs,
                    ResolverType::Rust,
                    ResolverType::Nodejs,
                ],
            )
            .unwrap();

        assert_eq!(
            discovery.resolvers,
            [ResolverType::Rust, ResolverType::Nodejs]
        );
        assert_eq!(
            discovery.packages,
            [
                DiscoveredPackage {
                    id: PackageId::new("node-app"),
                    ecosystem: EcosystemId::NODE,
                    path: Utf8PathBuf::from("."),
                },
                DiscoveredPackage {
                    id: PackageId::new("rust-app"),
                    ecosystem: EcosystemId::RUST,
                    path: Utf8PathBuf::from("."),
                },
            ]
        );
    }

    #[test]
    fn creates_minimal_default_package_configs() {
        let discovery = PackageDiscovery {
            resolvers: vec![ResolverType::Rust],
            packages: vec![DiscoveredPackage {
                id: PackageId::new("app"),
                ecosystem: EcosystemId::RUST,
                path: Utf8PathBuf::from("crates/app"),
            }],
        };

        let configs = discovery.default_package_configs().unwrap();

        assert_eq!(configs["app"].path, PathBuf::from("crates/app"));
        assert_eq!(configs["app"].resolver, ResolverType::Rust);
        assert!(matches!(configs["app"].channel, ReleaseChannel::Stable));
        assert!(configs["app"].assets.is_empty());
    }

    #[test]
    fn rejects_duplicate_ids_when_building_config_tables() {
        let discovery = PackageDiscovery {
            resolvers: vec![ResolverType::Rust, ResolverType::Nodejs],
            packages: vec![
                DiscoveredPackage {
                    id: PackageId::new("shared"),
                    ecosystem: EcosystemId::RUST,
                    path: Utf8PathBuf::from("rust"),
                },
                DiscoveredPackage {
                    id: PackageId::new("shared"),
                    ecosystem: EcosystemId::NODE,
                    path: Utf8PathBuf::from("node"),
                },
            ],
        };

        assert!(matches!(
            discovery.default_package_configs(),
            Err(PackageDiscoveryError::DuplicatePackageId { package })
                if package == PackageId::new("shared")
        ));
    }

    #[test]
    fn reports_dynamic_ecosystems_until_the_plugin_registry_is_connected() {
        let ecosystem = EcosystemId::new("com.example.engine").unwrap();
        let discovery = PackageDiscovery {
            resolvers: Vec::new(),
            packages: vec![DiscoveredPackage {
                id: PackageId::new("game"),
                ecosystem: ecosystem.clone(),
                path: Utf8PathBuf::from("game"),
            }],
        };

        assert!(matches!(
            discovery.default_package_configs(),
            Err(PackageDiscoveryError::ResolverUnavailable { ecosystem: actual })
                if actual == ecosystem
        ));
    }

    #[test]
    fn fails_the_complete_discovery_when_one_workspace_package_is_invalid() {
        let root = TemporaryRoot::new("invalid-member");
        fs::write(
            root.0.join("package.json"),
            r#"{"name":"root","version":"1.0.0","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        fs::create_dir_all(root.0.join("packages/broken")).unwrap();
        fs::write(
            root.0.join("packages/broken/package.json"),
            r#"{"name":"broken","version":"not-semver"}"#,
        )
        .unwrap();

        assert!(matches!(
            PackageDiscoveryService::default().discover(&root.0, &[ResolverType::Nodejs]),
            Err(PackageDiscoveryError::Adapter {
                resolver: ResolverType::Nodejs,
                ..
            })
        ));
    }
}
