use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

use semifold_core::{DiscoveredPackage, EcosystemId, PackageId};
use semifold_resolver::{
    adapter::{AdapterError, EcosystemAdapter},
    config::{Config, ConfigValidationError, PackageConfig, ReleaseChannel},
    plugin::{
        registry::{PluginRegistry, PluginRegistryError},
        runtime::BoaPluginRuntime,
    },
    resolver::{
        ResolverType, cpp::CppResolver, nodejs::NodejsResolver, python::PythonResolver,
        rust::RustResolver,
    },
};

use crate::package_path::{PackagePathError, normalize_package_path};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageDiscovery {
    pub resolvers: Vec<EcosystemId>,
    pub packages: Vec<DiscoveredPackage>,
}

impl PackageDiscovery {
    pub fn default_package_configs(
        &self,
    ) -> Result<BTreeMap<String, PackageConfig>, PackageDiscoveryError> {
        let mut packages = self.packages.iter().collect::<Vec<_>>();
        packages.sort();

        let mut packages_by_id = BTreeMap::<PackageId, Vec<&DiscoveredPackage>>::new();
        for package in packages {
            packages_by_id
                .entry(package.id.clone())
                .or_default()
                .push(package);
        }

        for (package_id, packages) in &packages_by_id {
            let mut ecosystems = BTreeSet::new();
            for package in packages {
                if !ecosystems.insert(package.ecosystem.clone()) {
                    return Err(PackageDiscoveryError::DuplicatePackageId {
                        package: package_id.clone(),
                    });
                }
            }
        }

        let mut occupied_ids = packages_by_id
            .iter()
            .filter(|(_, packages)| packages.len() == 1)
            .map(|(package_id, _)| package_id.to_string())
            .collect::<BTreeSet<_>>();
        let mut configs = BTreeMap::new();
        for (package_id, packages) in packages_by_id {
            let has_cross_ecosystem_duplicate = packages.len() > 1;
            for package in packages {
                let config_id = if has_cross_ecosystem_duplicate {
                    let base = format!("{}-{package_id}", package.ecosystem.as_str());
                    allocate_config_id(&base, &occupied_ids, self.packages.len()).ok_or_else(
                        || PackageDiscoveryError::PackageIdAllocationExhausted {
                            package: package_id.clone(),
                            ecosystem: package.ecosystem.clone(),
                        },
                    )?
                } else {
                    package_id.to_string()
                };
                occupied_ids.insert(config_id.clone());
                configs.insert(config_id, default_package_config(package));
            }
        }
        Ok(configs)
    }
}

fn allocate_config_id(
    base: &str,
    occupied_ids: &BTreeSet<String>,
    package_count: usize,
) -> Option<String> {
    if !occupied_ids.contains(base) {
        return Some(base.to_owned());
    }

    let max_suffix = package_count.checked_add(1)?;
    (2..=max_suffix)
        .map(|suffix| format!("{base}-{suffix}"))
        .find(|candidate| !occupied_ids.contains(candidate))
}

fn default_package_config(package: &DiscoveredPackage) -> PackageConfig {
    PackageConfig {
        path: PathBuf::from(package.path.as_str()),
        resolver: package.ecosystem.clone(),
        publish: None,
        channel: ReleaseChannel::Stable,
        channel_bump: None,
        assets: vec![],
        github_release: None,
        depends_on: vec![],
    }
}

#[derive(Default)]
pub struct ResolverRegistry {
    plugins: Option<PluginRegistry>,
}

impl ResolverRegistry {
    pub fn normalize_selection(resolvers: &[ResolverType]) -> Vec<ResolverType> {
        let mut resolvers = resolvers.to_vec();
        resolvers.sort();
        resolvers.dedup();
        resolvers
    }

    pub fn load(project_root: &Path, config: &Config) -> Result<Self, ResolverRegistryError> {
        let definitions = config.plugin_definitions()?;
        if definitions.is_empty() {
            return Ok(Self::default());
        }
        let project_root = camino::Utf8PathBuf::from_path_buf(project_root.to_path_buf())
            .map_err(|path| ResolverRegistryError::NonUtf8ProjectRoot { path })?;
        let plugins = PluginRegistry::load_with_reqwest(
            project_root,
            definitions,
            BoaPluginRuntime::default(),
        )?;
        Ok(Self {
            plugins: Some(plugins),
        })
    }

    pub fn create_adapter(
        &self,
        ecosystem: &EcosystemId,
    ) -> Result<Box<dyn EcosystemAdapter>, ResolverRegistryError> {
        let adapter: Box<dyn EcosystemAdapter> = match ecosystem.as_str() {
            "rust" => Box::new(RustResolver),
            "nodejs" => Box::new(NodejsResolver),
            "python" => Box::new(PythonResolver),
            "cpp" => Box::new(CppResolver),
            _ => {
                let plugin = self
                    .plugins
                    .as_ref()
                    .and_then(|plugins| plugins.get(ecosystem))
                    .ok_or_else(|| ResolverRegistryError::AdapterUnavailable {
                        ecosystem: ecosystem.clone(),
                    })?;
                Box::new(plugin.clone())
            }
        };
        Ok(adapter)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolverRegistryError {
    #[error("resolver registry project root is not UTF-8: {path:?}")]
    NonUtf8ProjectRoot { path: PathBuf },
    #[error(transparent)]
    Config(Box<ConfigValidationError>),
    #[error(transparent)]
    Plugin(Box<PluginRegistryError>),
    #[error("no adapter is registered for ecosystem {ecosystem}")]
    AdapterUnavailable { ecosystem: EcosystemId },
}

impl From<ConfigValidationError> for ResolverRegistryError {
    fn from(source: ConfigValidationError) -> Self {
        Self::Config(Box::new(source))
    }
}

impl From<PluginRegistryError> for ResolverRegistryError {
    fn from(source: PluginRegistryError) -> Self {
        Self::Plugin(Box::new(source))
    }
}

#[derive(Default)]
pub struct PackageDiscoveryService {
    registry: ResolverRegistry,
}

impl PackageDiscoveryService {
    pub fn from_config(
        project_root: &Path,
        config: &Config,
    ) -> Result<Self, PackageDiscoveryError> {
        Ok(Self {
            registry: ResolverRegistry::load(project_root, config)
                .map_err(PackageDiscoveryError::Registry)?,
        })
    }

    pub fn discover(
        &self,
        project_root: &Path,
        ecosystems: &[EcosystemId],
    ) -> Result<PackageDiscovery, PackageDiscoveryError> {
        let mut resolvers = ecosystems.to_vec();
        resolvers.sort();
        resolvers.dedup();
        let mut packages = Vec::new();
        for ecosystem in &resolvers {
            let root = camino::Utf8Path::from_path(project_root).ok_or_else(|| {
                PackageDiscoveryError::InvalidProjectRoot {
                    path: project_root.to_path_buf(),
                }
            })?;
            let mut discovered = self
                .registry
                .create_adapter(ecosystem)
                .map_err(PackageDiscoveryError::Registry)?
                .discover(root)
                .map_err(|source| PackageDiscoveryError::Adapter {
                    ecosystem: ecosystem.clone(),
                    source,
                })?
                .into_iter()
                .map(|package| {
                    let path = normalize_package_path(project_root, package.path.as_std_path())
                        .map_err(|source| PackageDiscoveryError::PackagePath {
                            ecosystem: ecosystem.clone(),
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
        ecosystem: EcosystemId,
        source: AdapterError,
    },
    InvalidProjectRoot {
        path: PathBuf,
    },
    PackagePath {
        ecosystem: EcosystemId,
        package: String,
        source: PackagePathError,
    },
    DuplicatePackageId {
        package: PackageId,
    },
    PackageIdAllocationExhausted {
        package: PackageId,
        ecosystem: EcosystemId,
    },
    Registry(ResolverRegistryError),
}

impl fmt::Display for PackageDiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter { ecosystem, source } => {
                write!(formatter, "{ecosystem} package discovery failed: {source}")
            }
            Self::InvalidProjectRoot { path } => {
                write!(
                    formatter,
                    "package discovery project root is not valid UTF-8: {}",
                    path.display()
                )
            }
            Self::PackagePath {
                ecosystem,
                package,
                source,
            } => write!(
                formatter,
                "{ecosystem} package discovery returned an invalid path for {package}: {source}"
            ),
            Self::DuplicatePackageId { package } => {
                write!(
                    formatter,
                    "package discovery returned duplicate package id: {package}"
                )
            }
            Self::PackageIdAllocationExhausted { package, ecosystem } => write!(
                formatter,
                "package discovery could not allocate a unique id for {ecosystem} package: {package}"
            ),
            Self::Registry(source) => source.fmt(formatter),
        }
    }
}

impl Error for PackageDiscoveryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Adapter { source, .. } => Some(source),
            Self::PackagePath { source, .. } => Some(source),
            Self::Registry(source) => Some(source),
            Self::InvalidProjectRoot { .. }
            | Self::DuplicatePackageId { .. }
            | Self::PackageIdAllocationExhausted { .. } => None,
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
                &[EcosystemId::NODE, EcosystemId::RUST, EcosystemId::NODE],
            )
            .unwrap();

        assert_eq!(discovery.resolvers, [EcosystemId::RUST, EcosystemId::NODE]);
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
            resolvers: vec![EcosystemId::RUST],
            packages: vec![DiscoveredPackage {
                id: PackageId::new("app"),
                ecosystem: EcosystemId::RUST,
                path: Utf8PathBuf::from("crates/app"),
            }],
        };

        let configs = discovery.default_package_configs().unwrap();

        assert_eq!(configs["app"].path, PathBuf::from("crates/app"));
        assert_eq!(configs["app"].resolver, EcosystemId::RUST);
        assert!(matches!(configs["app"].channel, ReleaseChannel::Stable));
        assert!(configs["app"].assets.is_empty());
    }

    #[test]
    fn qualifies_cross_ecosystem_duplicate_ids_when_building_config_tables() {
        let discovery = PackageDiscovery {
            resolvers: vec![EcosystemId::RUST, EcosystemId::NODE],
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

        let configs = discovery.default_package_configs().unwrap();
        assert_eq!(
            configs.keys().map(String::as_str).collect::<Vec<_>>(),
            ["nodejs-shared", "rust-shared"]
        );
        assert_eq!(configs["nodejs-shared"].path, PathBuf::from("node"));
        assert_eq!(configs["rust-shared"].path, PathBuf::from("rust"));
    }

    #[test]
    fn rejects_duplicate_ids_within_one_ecosystem() {
        let discovery = PackageDiscovery {
            resolvers: vec![EcosystemId::RUST],
            packages: vec![
                DiscoveredPackage {
                    id: PackageId::new("shared"),
                    ecosystem: EcosystemId::RUST,
                    path: Utf8PathBuf::from("crates/first"),
                },
                DiscoveredPackage {
                    id: PackageId::new("shared"),
                    ecosystem: EcosystemId::RUST,
                    path: Utf8PathBuf::from("crates/second"),
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
    fn appends_a_deterministic_suffix_when_qualified_ids_already_exist() {
        let discovery = PackageDiscovery {
            resolvers: vec![EcosystemId::NODE, EcosystemId::RUST],
            packages: vec![
                DiscoveredPackage {
                    id: PackageId::new("rust-shared-2"),
                    ecosystem: EcosystemId::RUST,
                    path: Utf8PathBuf::from("crates/existing-two"),
                },
                DiscoveredPackage {
                    id: PackageId::new("shared"),
                    ecosystem: EcosystemId::NODE,
                    path: Utf8PathBuf::from("packages/shared"),
                },
                DiscoveredPackage {
                    id: PackageId::new("rust-shared"),
                    ecosystem: EcosystemId::RUST,
                    path: Utf8PathBuf::from("crates/existing"),
                },
                DiscoveredPackage {
                    id: PackageId::new("shared"),
                    ecosystem: EcosystemId::RUST,
                    path: Utf8PathBuf::from("crates/shared"),
                },
            ],
        };

        let configs = discovery.default_package_configs().unwrap();
        assert_eq!(
            configs.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "nodejs-shared",
                "rust-shared",
                "rust-shared-2",
                "rust-shared-3"
            ]
        );
        assert_eq!(
            configs["rust-shared-3"].path,
            PathBuf::from("crates/shared")
        );
    }

    #[test]
    fn creates_default_package_configs_for_dynamic_ecosystems() {
        let ecosystem = EcosystemId::new("com.example.engine").unwrap();
        let discovery = PackageDiscovery {
            resolvers: Vec::new(),
            packages: vec![DiscoveredPackage {
                id: PackageId::new("game"),
                ecosystem: ecosystem.clone(),
                path: Utf8PathBuf::from("game"),
            }],
        };

        let configs = discovery.default_package_configs().unwrap();
        assert_eq!(configs["game"].resolver, ecosystem);
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
            PackageDiscoveryService::default().discover(&root.0, &[EcosystemId::NODE]),
            Err(PackageDiscoveryError::Adapter {
                ecosystem,
                ..
            }) if ecosystem == EcosystemId::NODE
        ));
    }
}
