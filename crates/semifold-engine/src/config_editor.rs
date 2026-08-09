use std::{error::Error, fmt, fs, io};

use camino::{Utf8Path, Utf8PathBuf};
use semifold_core::{ConfigSyncPlan, DiscoveredPackage, EcosystemId, PackageId};
use semifold_resolver::config::Config;
use toml_edit::{DocumentMut, Item, Table, value};

/// Preserves an existing TOML configuration document while applying package-only sync edits.
pub struct TomlConfigEditor {
    path: Utf8PathBuf,
    document: DocumentMut,
}

impl TomlConfigEditor {
    pub fn load(path: &Utf8Path) -> Result<Self, ConfigEditError> {
        let content = fs::read_to_string(path).map_err(|source| ConfigEditError::Read {
            path: path.to_owned(),
            source,
        })?;
        let document = content
            .parse::<DocumentMut>()
            .map_err(|source| ConfigEditError::Parse {
                path: path.to_owned(),
                reason: source.to_string(),
            })?;
        let editor = Self {
            path: path.to_owned(),
            document,
        };
        editor.validate()?;
        Ok(editor)
    }

    pub fn validate(&self) -> Result<Config, ConfigEditError> {
        toml_edit::de::from_str(&self.document.to_string()).map_err(|source| {
            ConfigEditError::InvalidConfig {
                path: self.path.clone(),
                reason: source.to_string(),
            }
        })
    }

    pub fn apply(&mut self, plan: &ConfigSyncPlan) -> Result<(), ConfigEditError> {
        if plan.config_path != self.path {
            return Err(ConfigEditError::PlanPathMismatch {
                editor: self.path.clone(),
                plan: plan.config_path.clone(),
            });
        }
        if !plan.conflicts.is_empty() {
            return Err(ConfigEditError::ConflictingPlan);
        }

        let packages = self.packages_mut()?;
        for rename in &plan.renamed {
            if packages.contains_key(rename.to.as_str()) {
                return Err(ConfigEditError::PackageAlreadyExists {
                    package: rename.to.clone(),
                });
            }
            let package = packages.remove(rename.from.as_str()).ok_or_else(|| {
                ConfigEditError::PackageNotFound {
                    package: rename.from.clone(),
                }
            })?;
            packages.insert(rename.to.as_str(), package);
        }
        for moved in &plan.moved {
            let package = package_table_mut(packages, &moved.package)?;
            package.insert("path", value(moved.to.as_str()));
        }
        let mut added = plan.added.iter().collect::<Vec<_>>();
        added.sort_by(|left, right| left.id.cmp(&right.id));
        for added in added {
            insert_discovered_package(packages, added)?;
        }
        if plan.prune_missing {
            for missing in &plan.missing {
                packages.remove(missing.id.as_str()).ok_or_else(|| {
                    ConfigEditError::PackageNotFound {
                        package: missing.id.clone(),
                    }
                })?;
            }
        }

        self.validate()?;
        Ok(())
    }

    #[must_use]
    pub fn render(&self) -> String {
        self.document.to_string()
    }

    fn packages_mut(&mut self) -> Result<&mut Table, ConfigEditError> {
        self.document
            .get_mut("packages")
            .and_then(Item::as_table_mut)
            .ok_or(ConfigEditError::MissingPackagesTable)
    }
}

fn package_table_mut<'a>(
    packages: &'a mut Table,
    package: &PackageId,
) -> Result<&'a mut dyn toml_edit::TableLike, ConfigEditError> {
    packages
        .get_mut(package.as_str())
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| ConfigEditError::PackageNotFound {
            package: package.clone(),
        })
}

fn insert_discovered_package(
    packages: &mut Table,
    package: &DiscoveredPackage,
) -> Result<(), ConfigEditError> {
    if packages.contains_key(package.id.as_str()) {
        return Err(ConfigEditError::PackageAlreadyExists {
            package: package.id.clone(),
        });
    }

    let mut table = Table::new();
    table.insert("path", value(package.path.as_str()));
    table.insert("resolver", value(resolver_name(&package.ecosystem)));
    packages.insert(package.id.as_str(), Item::Table(table));
    Ok(())
}

fn resolver_name(ecosystem: &EcosystemId) -> &str {
    ecosystem.as_str()
}

#[derive(Debug)]
pub enum ConfigEditError {
    Read {
        path: Utf8PathBuf,
        source: io::Error,
    },
    Parse {
        path: Utf8PathBuf,
        reason: String,
    },
    InvalidConfig {
        path: Utf8PathBuf,
        reason: String,
    },
    MissingPackagesTable,
    PlanPathMismatch {
        editor: Utf8PathBuf,
        plan: Utf8PathBuf,
    },
    ConflictingPlan,
    PackageNotFound {
        package: PackageId,
    },
    PackageAlreadyExists {
        package: PackageId,
    },
}

impl fmt::Display for ConfigEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(formatter, "failed to read {path}: {source}"),
            Self::Parse { path, reason } => write!(formatter, "failed to parse {path}: {reason}"),
            Self::InvalidConfig { path, reason } => {
                write!(formatter, "invalid config {path}: {reason}")
            }
            Self::MissingPackagesTable => formatter.write_str("config is missing [packages]"),
            Self::PlanPathMismatch { editor, plan } => {
                write!(
                    formatter,
                    "config sync plan for {plan} cannot edit {editor}"
                )
            }
            Self::ConflictingPlan => {
                formatter.write_str("config sync plan contains conflicts and cannot be applied")
            }
            Self::PackageNotFound { package } => {
                write!(formatter, "package {package} is not configured")
            }
            Self::PackageAlreadyExists { package } => {
                write!(formatter, "package {package} is already configured")
            }
        }
    }
}

impl Error for ConfigEditError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use camino::Utf8PathBuf;
    use semifold_core::{
        ConfigConflict, ConfigSyncPlan, ConfiguredPackage, DiscoveredPackage, EcosystemId,
        PackageId, PackageMove, PackageRename,
    };

    use super::TomlConfigEditor;

    static NEXT_TEMPORARY_CONFIG: AtomicU64 = AtomicU64::new(0);

    const CONFIG: &str = r#"# top-level comment
[branches]
base = "main"
release = "release"

[tags]

[changelog]
template = "Release {{ package.next_version }}"
changeset-template = "Change {{ changeset.summary }}"

[resolver.rust.pre-check]
type = "http"
url = ""

[release]
# release policy must remain untouched
strategy = "fixed"

[[release.units]]
name = "all"

[packages.old-name]
# retain this comment and every manual field
path = "crates/app"
resolver = "rust"
channel = "stable"
assets = ["README.md"]
github-release = true
depends-on = ["core"]
plugin-option = true

[packages.moved]
path = "crates/old-location"
resolver = "rust"

[packages.keep]
path = "crates/keep"
resolver = "rust"
custom = "preserved"
"#;

    fn temporary_config_path() -> Utf8PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Utf8PathBuf::from_path_buf(std::env::temp_dir().join(format!(
            "semifold-config-editor-{}-{nonce}-{}.toml",
            std::process::id(),
            NEXT_TEMPORARY_CONFIG.fetch_add(1, Ordering::Relaxed)
        )))
        .unwrap()
    }

    fn plan(path: Utf8PathBuf) -> ConfigSyncPlan {
        ConfigSyncPlan {
            config_path: path,
            prune_missing: false,
            added: vec![DiscoveredPackage {
                id: PackageId::new("new-package"),
                ecosystem: EcosystemId::PYTHON,
                path: Utf8PathBuf::from("packages/new-package"),
            }],
            missing: vec![],
            renamed: vec![PackageRename {
                from: PackageId::new("old-name"),
                to: PackageId::new("renamed-package"),
                ecosystem: EcosystemId::RUST,
                path: Utf8PathBuf::from("crates/app"),
            }],
            moved: vec![PackageMove {
                package: PackageId::new("moved"),
                ecosystem: EcosystemId::RUST,
                from: Utf8PathBuf::from("crates/old-location"),
                to: Utf8PathBuf::from("crates/new-location"),
            }],
            conflicts: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn applies_package_changes_without_touching_manual_or_release_configuration() {
        let path = temporary_config_path();
        fs::write(&path, CONFIG).unwrap();
        let mut editor = TomlConfigEditor::load(&path).unwrap();
        editor.apply(&plan(path.clone())).unwrap();
        let rendered = editor.render();

        assert!(rendered.contains("# top-level comment"));
        assert!(rendered.contains("# release policy must remain untouched"));
        assert!(rendered.contains("template = \"Release {{ package.next_version }}\""));
        assert!(rendered.contains("changeset-template = \"Change {{ changeset.summary }}\""));
        assert!(rendered.contains("[[release.units]]\nname = \"all\""));
        assert!(rendered.contains("[packages.renamed-package]"));
        assert!(rendered.contains("# retain this comment and every manual field"));
        assert!(rendered.contains("channel = \"stable\""));
        assert!(rendered.contains("assets = [\"README.md\"]"));
        assert!(rendered.contains("github-release = true"));
        assert!(rendered.contains("depends-on = [\"core\"]"));
        assert!(rendered.contains("plugin-option = true"));
        assert!(rendered.contains("path = \"crates/new-location\""));
        assert!(rendered.contains(
            "[packages.new-package]\npath = \"packages/new-package\"\nresolver = \"python\""
        ));
        assert!(rendered.contains(
            "[packages.keep]\npath = \"crates/keep\"\nresolver = \"rust\"\ncustom = \"preserved\""
        ));
        assert!(!rendered.contains("[packages.old-name]"));
        assert_eq!(editor.validate().unwrap().packages.len(), 4);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_conflicting_plans_without_changing_the_document() {
        let path = temporary_config_path();
        fs::write(&path, CONFIG).unwrap();
        let mut editor = TomlConfigEditor::load(&path).unwrap();
        let mut plan = plan(path.clone());
        plan.conflicts.push(ConfigConflict::AmbiguousMatch {
            configured: vec![],
            discovered: vec![],
        });

        assert!(editor.apply(&plan).is_err());
        assert_eq!(editor.render(), CONFIG);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn leaves_missing_packages_in_place_until_prune_is_requested() {
        let path = temporary_config_path();
        fs::write(&path, CONFIG).unwrap();
        let mut editor = TomlConfigEditor::load(&path).unwrap();
        let mut plan = plan(path.clone());
        plan.added.clear();
        plan.renamed.clear();
        plan.moved.clear();
        plan.missing = vec![ConfiguredPackage {
            id: PackageId::new("keep"),
            ecosystem: EcosystemId::RUST,
            path: Utf8PathBuf::from("crates/keep"),
        }];

        editor.apply(&plan).unwrap();
        assert_eq!(editor.render(), CONFIG);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn validates_the_original_document_when_loading() {
        let path = temporary_config_path();
        fs::write(&path, "[packages.invalid]\npath = \"crates/invalid\"\n").unwrap();

        assert!(TomlConfigEditor::load(&path).is_err());

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn inserts_new_packages_by_id_without_reordering_existing_tables() {
        let path = temporary_config_path();
        fs::write(&path, CONFIG).unwrap();
        let mut editor = TomlConfigEditor::load(&path).unwrap();
        let mut plan = plan(path.clone());
        plan.renamed.clear();
        plan.moved.clear();
        plan.added = vec![
            DiscoveredPackage {
                id: PackageId::new("zeta"),
                ecosystem: EcosystemId::RUST,
                path: Utf8PathBuf::from("crates/zeta"),
            },
            DiscoveredPackage {
                id: PackageId::new("alpha"),
                ecosystem: EcosystemId::RUST,
                path: Utf8PathBuf::from("crates/alpha"),
            },
        ];

        editor.apply(&plan).unwrap();
        let rendered = editor.render();
        let old_name = rendered.find("[packages.old-name]").unwrap();
        let moved = rendered.find("[packages.moved]").unwrap();
        let alpha = rendered.find("[packages.alpha]").unwrap();
        let zeta = rendered.find("[packages.zeta]").unwrap();

        assert!(old_name < moved);
        assert!(moved < alpha);
        assert!(alpha < zeta);

        fs::remove_file(path).unwrap();
    }
}
