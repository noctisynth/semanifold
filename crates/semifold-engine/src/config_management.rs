use std::collections::BTreeSet;

use camino::Utf8PathBuf;
use semifold_resolver::{config, error::ResolveError};
use thiserror::Error;
use toml_edit::{DocumentMut, Item, Table, TableLike, Value, value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigMutationPlan {
    pub path: Utf8PathBuf,
    pub content: String,
    pub packages: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelUpdate {
    pub channel: Option<String>,
    pub bump: Option<config::ChannelBump>,
    pub packages: Vec<String>,
    pub all: bool,
}

pub fn plan_config_migration(
    path: Utf8PathBuf,
    content: &str,
) -> Result<ConfigMutationPlan, ConfigMutationError> {
    let mut document = parse_document(&path, content)?;
    let mut migrated = BTreeSet::new();
    migrate_snake_case_fields(document.as_table_mut(), &mut migrated)?;
    migrate_pre_check_types(document.as_table_mut(), &mut migrated);
    let packages = document
        .get_mut("packages")
        .and_then(Item::as_table_mut)
        .ok_or(ConfigMutationError::MissingPackagesTable)?;

    for (name, package) in packages.iter_mut() {
        let table =
            package
                .as_table_like_mut()
                .ok_or_else(|| ConfigMutationError::PackageNotTable {
                    package: name.to_string(),
                })?;
        let has_channel = table.contains_key("channel");
        let legacy = table.get("version-mode").map(ToString::to_string);
        let Some(legacy) = legacy else {
            continue;
        };
        if has_channel {
            return Err(ConfigMutationError::ChannelLegacyConflict {
                package: name.to_string(),
            });
        }

        let version_mode = parse_legacy_version_mode(&legacy).map_err(|reason| {
            ConfigMutationError::InvalidLegacyVersionMode {
                package: name.to_string(),
                reason,
            }
        })?;
        table.remove("version-mode");
        if let config::VersionMode::PreRelease { tag } = version_mode {
            table.insert("channel", value(tag));
        }
        migrated.insert(name.to_string());
    }

    let content = document.to_string();
    Ok(ConfigMutationPlan {
        path,
        content,
        packages: migrated.into_iter().collect(),
    })
}

fn migrate_pre_check_types(document: &mut Table, migrated: &mut BTreeSet<String>) {
    let Some(resolvers) = document
        .get_mut("resolver")
        .and_then(Item::as_table_like_mut)
    else {
        return;
    };
    for (name, resolver) in resolvers.iter_mut() {
        let Some(pre_check) = resolver
            .as_table_like_mut()
            .and_then(|resolver| resolver.get_mut("pre-check"))
            .and_then(Item::as_table_like_mut)
        else {
            continue;
        };
        if !pre_check.contains_key("type") && pre_check.contains_key("url") {
            pre_check.insert("type", value("http"));
            migrated.insert(format!("resolver.{name}.pre-check"));
        }
    }
}

pub fn plan_channel_update(
    path: Utf8PathBuf,
    content: &str,
    update: &ChannelUpdate,
) -> Result<ConfigMutationPlan, ConfigMutationError> {
    let mut document = parse_document(&path, content)?;
    let packages = document
        .get_mut("packages")
        .and_then(Item::as_table_mut)
        .ok_or(ConfigMutationError::MissingPackagesTable)?;
    let targets = if update.all {
        packages.iter().map(|(name, _)| name.to_string()).collect()
    } else {
        update.packages.clone()
    };

    for name in &targets {
        if !packages.contains_key(name) {
            return Err(ConfigMutationError::PackageNotConfigured {
                package: name.clone(),
            });
        }
    }

    let mut updated = Vec::new();
    for name in targets {
        let table = packages
            .get_mut(&name)
            .ok_or_else(|| ConfigMutationError::PackageNotConfigured {
                package: name.clone(),
            })?
            .as_table_like_mut()
            .ok_or_else(|| ConfigMutationError::PackageNotTable {
                package: name.clone(),
            })?;
        let current = table
            .get("channel")
            .and_then(Item::as_value)
            .and_then(Value::as_str);
        if current == update.channel.as_deref() {
            continue;
        }
        match update.channel.as_deref() {
            Some(channel) => {
                table.insert("channel", value(channel));
                match update.bump {
                    Some(bump) => {
                        table.insert("channel-bump", value(channel_bump_name(bump)));
                    }
                    None => {
                        table.remove("channel-bump");
                    }
                }
            }
            None => {
                table.remove("channel");
                table.remove("channel-bump");
            }
        }
        updated.push(name);
    }

    let content = document.to_string();
    Ok(ConfigMutationPlan {
        path,
        content,
        packages: updated,
    })
}

fn parse_document(path: &Utf8PathBuf, content: &str) -> Result<DocumentMut, ConfigMutationError> {
    content
        .parse::<DocumentMut>()
        .map_err(|source| ConfigMutationError::Parse {
            path: path.clone(),
            source,
        })
}

const SNAKE_CASE_FIELDS: [(&str, &str); 9] = [
    ("version_mode", "version-mode"),
    ("channel_bump", "channel-bump"),
    ("depends_on", "depends-on"),
    ("github_release", "github-release"),
    ("pre_check", "pre-check"),
    ("post_version", "post-version"),
    ("extra_headers", "extra-headers"),
    ("extra_env", "extra-env"),
    ("dry_run", "dry-run"),
];

fn migrate_snake_case_fields(
    document: &mut Table,
    migrated: &mut BTreeSet<String>,
) -> Result<(), ConfigMutationError> {
    if let Some(packages) = document
        .get_mut("packages")
        .and_then(Item::as_table_like_mut)
    {
        for (name, package) in packages.iter_mut() {
            if let Some(package) = package.as_table_like_mut() {
                rename_table_fields(
                    package,
                    &format!("packages.{name}"),
                    &SNAKE_CASE_FIELDS[..4],
                    migrated,
                )?;
            }
        }
    }

    if let Some(resolvers) = document
        .get_mut("resolver")
        .and_then(Item::as_table_like_mut)
    {
        for (name, resolver) in resolvers.iter_mut() {
            let Some(resolver) = resolver.as_table_like_mut() else {
                continue;
            };
            let scope = format!("resolver.{name}");
            rename_table_fields(resolver, &scope, &SNAKE_CASE_FIELDS[4..6], migrated)?;
            if let Some(pre_check) = resolver
                .get_mut("pre-check")
                .and_then(Item::as_table_like_mut)
            {
                rename_table_fields(
                    pre_check,
                    &format!("{scope}.pre-check"),
                    &SNAKE_CASE_FIELDS[6..7],
                    migrated,
                )?;
            }
            for phase in ["prepublish", "publish", "post-version"] {
                if let Some(commands) = resolver.get_mut(phase) {
                    migrate_command_fields(commands, &format!("{scope}.{phase}"), migrated)?;
                }
            }
        }
    }
    Ok(())
}

fn migrate_command_fields(
    commands: &mut Item,
    scope: &str,
    migrated: &mut BTreeSet<String>,
) -> Result<(), ConfigMutationError> {
    match commands {
        Item::ArrayOfTables(tables) => {
            for (index, command) in tables.iter_mut().enumerate() {
                rename_table_fields(
                    command,
                    &format!("{scope}[{index}]"),
                    &SNAKE_CASE_FIELDS[7..],
                    migrated,
                )?;
            }
        }
        Item::Value(Value::Array(commands)) => {
            for (index, command) in commands.iter_mut().enumerate() {
                if let Value::InlineTable(command) = command {
                    rename_table_fields(
                        command,
                        &format!("{scope}[{index}]"),
                        &SNAKE_CASE_FIELDS[7..],
                        migrated,
                    )?;
                }
            }
        }
        Item::Table(command) => {
            rename_table_fields(command, scope, &SNAKE_CASE_FIELDS[7..], migrated)?;
        }
        _ => {}
    }
    Ok(())
}

fn rename_table_fields(
    table: &mut dyn TableLike,
    scope: &str,
    fields: &[(&str, &str)],
    migrated: &mut BTreeSet<String>,
) -> Result<(), ConfigMutationError> {
    let renames = fields
        .iter()
        .filter(|(legacy, _)| table.contains_key(legacy))
        .copied()
        .collect::<Vec<_>>();

    for (legacy, current) in &renames {
        if table.contains_key(current) {
            return Err(ConfigMutationError::SnakeCaseConflict {
                scope: scope.to_string(),
                legacy: (*legacy).to_string(),
                current: (*current).to_string(),
            });
        }
        migrated.insert(format!("{scope}.{legacy}"));
    }

    if !renames.is_empty() {
        let entries = table
            .iter()
            .filter_map(|(name, item)| {
                let key = table.key(name)?;
                let renamed = renames
                    .iter()
                    .find_map(|(legacy, current)| (*legacy == name).then_some(*current))
                    .unwrap_or(name);
                Some((
                    toml_edit::Key::new(renamed)
                        .with_leaf_decor(key.leaf_decor().clone())
                        .with_dotted_decor(key.dotted_decor().clone()),
                    item.clone(),
                ))
            })
            .collect::<Vec<_>>();
        table.clear();
        for (key, item) in entries {
            table.entry_format(&key).or_insert(item);
        }
    }
    Ok(())
}

fn parse_legacy_version_mode(value: &str) -> Result<config::VersionMode, String> {
    #[derive(serde::Deserialize)]
    struct LegacyVersionMode {
        #[serde(rename = "version-mode")]
        version_mode: config::VersionMode,
    }

    toml_edit::de::from_str::<LegacyVersionMode>(&format!("version-mode = {value}"))
        .map(|legacy| legacy.version_mode)
        .map_err(|error| error.to_string())
}

const fn channel_bump_name(bump: config::ChannelBump) -> &'static str {
    match bump {
        config::ChannelBump::Preserve => "preserve",
        config::ChannelBump::Patch => "patch",
        config::ChannelBump::Minor => "minor",
        config::ChannelBump::Major => "major",
    }
}

#[derive(Debug, Error)]
pub enum ConfigMutationError {
    #[error("failed to read configuration {path}")]
    Read {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse configuration {path}")]
    Parse {
        path: Utf8PathBuf,
        #[source]
        source: toml_edit::TomlError,
    },
    #[error("configuration is missing the packages table")]
    MissingPackagesTable,
    #[error("package {package} must be a table")]
    PackageNotTable { package: String },
    #[error("package {package} contains both channel and version-mode")]
    ChannelLegacyConflict { package: String },
    #[error("package {package} has an invalid version-mode: {reason}")]
    InvalidLegacyVersionMode { package: String, reason: String },
    #[error("configuration scope {scope} contains both {legacy} and {current}")]
    SnakeCaseConflict {
        scope: String,
        legacy: String,
        current: String,
    },
    #[error("package {package} is not configured")]
    PackageNotConfigured { package: String },
    #[error("configuration is invalid after editing")]
    InvalidResult(#[source] ResolveError),
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::{ConfigMutationError, plan_config_migration};

    #[test]
    fn migrates_package_github_release_to_kebab_case() {
        let plan = plan_config_migration(
            Utf8PathBuf::from("config.toml"),
            r#"
[branches]
base = "main"
release = "release"

[tags]

[packages.app]
path = "."
resolver = "rust"
github_release = true

[resolver.rust]
"#,
        )
        .expect("Package GitHub Release policy must migrate");

        assert!(plan.content.contains("github-release = true"));
        assert!(!plan.content.contains("github_release"));
    }

    #[test]
    fn rejects_conflicting_package_github_release_fields() {
        let error = plan_config_migration(
            Utf8PathBuf::from("config.toml"),
            r#"
[packages.app]
path = "."
resolver = "rust"
github_release = true
github-release = false
"#,
        )
        .expect_err("Conflicting GitHub Release policies must fail");

        assert!(matches!(
            error,
            ConfigMutationError::SnakeCaseConflict { .. }
        ));
    }

    #[test]
    fn migrates_legacy_http_pre_check_to_explicit_type() {
        let plan = plan_config_migration(
            Utf8PathBuf::from("config.toml"),
            r#"
[packages]

[resolver.rust.pre-check]
url = "https://registry.test/{{ package.name }}/{{ package.version }}"
"#,
        )
        .expect("legacy HTTP pre-check must migrate");

        assert!(plan.content.contains("type = \"http\""));
        assert!(
            plan.packages
                .contains(&"resolver.rust.pre-check".to_string())
        );
    }
}
