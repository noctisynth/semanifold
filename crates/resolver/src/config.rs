use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use camino::Utf8PathBuf;
use semifold_core::{EcosystemId, PackageId};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    error::ResolveError,
    plugin::{
        http::PluginHttpOrigin,
        registry::{PluginDefinition, PluginRegistryError},
    },
    resolver,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct BranchesConfig {
    pub base: String,
    pub release: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ReleaseConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request_title: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ChangelogConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changeset_template: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AssetConfig {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Asset {
    Asset(AssetConfig),
    String(String),
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum VersionMode {
    /// Semantic versioning mode.
    #[default]
    Semantic,
    /// Pre-release versioning mode.
    PreRelease {
        /// Pre-release tag.
        tag: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ReleaseChannel {
    #[default]
    Stable,
    Named(String),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelBump {
    Preserve,
    Patch,
    Minor,
    Major,
}

impl ReleaseChannel {
    pub fn is_stable(&self) -> bool {
        matches!(self, Self::Stable)
    }

    fn from_config_value(value: String) -> Result<Self, String> {
        match value.as_str() {
            "stable" => Ok(Self::Stable),
            "" => Err("release channel must not be empty".to_string()),
            _ => Ok(Self::Named(value)),
        }
    }
}

impl Serialize for ReleaseChannel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Stable => serializer.serialize_str("stable"),
            Self::Named(name) => serializer.serialize_str(name),
        }
    }
}

impl<'de> Deserialize<'de> for ReleaseChannel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)
            .and_then(|value| Self::from_config_value(value).map_err(serde::de::Error::custom))
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PackageConfig {
    /// Path to the package root directory.
    pub path: PathBuf,
    /// Resolver type to use.
    pub resolver: EcosystemId,
    /// Optional override for manifest- or plugin-derived publishability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish: Option<bool>,
    /// Release channel to use. Stable is the default and is omitted when saved.
    #[serde(default, skip_serializing_if = "ReleaseChannel::is_stable")]
    pub channel: ReleaseChannel,
    /// One-shot stable-base override for the next transition into `channel`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_bump: Option<ChannelBump>,
    /// Assets to publish.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<Asset>,
    /// Whether to create a GitHub Release for this package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_release: Option<bool>,
    /// Supplemental internal dependency edges keyed by stable package ID.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<PackageId>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageConfigInput {
    path: PathBuf,
    resolver: EcosystemId,
    #[serde(default)]
    publish: Option<bool>,
    #[serde(default)]
    channel: Option<ReleaseChannel>,
    #[serde(default)]
    channel_bump: Option<ChannelBump>,
    #[serde(default, rename = "version-mode")]
    legacy_version_mode: Option<VersionMode>,
    #[serde(default)]
    assets: Vec<Asset>,
    #[serde(default)]
    github_release: Option<bool>,
    #[serde(default)]
    depends_on: Vec<PackageId>,
}

impl<'de> Deserialize<'de> for PackageConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = PackageConfigInput::deserialize(deserializer)?;
        let channel = input.channel.unwrap_or(match input.legacy_version_mode {
            Some(VersionMode::PreRelease { tag }) => ReleaseChannel::Named(tag),
            Some(VersionMode::Semantic) | None => ReleaseChannel::Stable,
        });
        Ok(Self {
            path: input.path,
            resolver: input.resolver,
            publish: input.publish,
            channel,
            channel_bump: input.channel_bump,
            assets: input.assets,
            github_release: input.github_release,
            depends_on: input.depends_on,
        })
    }
}

impl PackageConfig {
    #[must_use]
    pub fn effective_publishable(&self, adapter_publishable: bool) -> bool {
        self.publish.unwrap_or(adapter_publishable)
    }

    #[must_use]
    pub fn github_release_enabled(&self, publishable: bool) -> bool {
        self.github_release.unwrap_or(publishable)
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum StdioType {
    #[default]
    Inherit,
    Pipe,
    Null,
}

impl StdioType {
    pub fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }
}

impl From<StdioType> for std::process::Stdio {
    fn from(value: StdioType) -> Self {
        match value {
            StdioType::Inherit => Self::inherit(),
            StdioType::Pipe => Self::piped(),
            StdioType::Null => Self::null(),
        }
    }
}

/// Configuration for a command to run.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct CommandConfig {
    /// Executable command to run.
    pub command: String,
    /// Arguments to pass to the command.
    pub args: Option<Vec<String>>,
    /// Environment variables to set before running the command.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_env: BTreeMap<String, String>,
    /// Type of standard output to use.
    #[serde(default, skip_serializing_if = "StdioType::is_inherit")]
    pub stdout: StdioType,
    /// Type of standard error to use.
    #[serde(default, skip_serializing_if = "StdioType::is_inherit")]
    pub stderr: StdioType,
    /// Whether to run the command in dry-run mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PreCheckConfig {
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        extra_headers: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        retry: Vec<u64>,
    },
    Command {
        command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        args: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        extra_env: BTreeMap<String, String>,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct ResolverConfig {
    /// Pre-check configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_check: Option<PreCheckConfig>,
    /// Commands to run before publish.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prepublish: Vec<CommandConfig>,
    /// Commands to run for publish.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publish: Vec<CommandConfig>,
    /// Commands to run after versioning.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub post_version: Vec<CommandConfig>,
}

/// Repository-local JavaScript plugin registration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginConfig {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_origins: BTreeSet<PluginHttpOrigin>,
}

impl PluginConfig {
    fn definition(
        &self,
        ecosystem: &EcosystemId,
    ) -> Result<PluginDefinition, ConfigValidationError> {
        let path = Utf8PathBuf::from_path_buf(self.path.clone()).map_err(|path| {
            ConfigValidationError::NonUtf8PluginPath {
                ecosystem: ecosystem.clone(),
                path,
            }
        })?;
        let definition = PluginDefinition::new(ecosystem.clone(), path).map_err(|source| {
            ConfigValidationError::PluginDefinition {
                ecosystem: ecosystem.clone(),
                source,
            }
        })?;
        let definition = if let Some(sha256) = &self.sha256 {
            definition.with_sha256(sha256).map_err(|source| {
                ConfigValidationError::PluginDefinition {
                    ecosystem: ecosystem.clone(),
                    source,
                }
            })?
        } else {
            definition
        };
        Ok(definition.with_allowed_origins(self.allowed_origins.iter().cloned()))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    /// Branch configuration.
    pub branches: BranchesConfig,
    /// Optional release commit and pull request templates.
    #[serde(default, skip_serializing_if = "is_default_release_config")]
    pub release: ReleaseConfig,
    /// Tag configuration.
    pub tags: BTreeMap<String, String>,
    /// Changelog template configuration.
    #[serde(default, skip_serializing_if = "is_default_changelog_config")]
    pub changelog: ChangelogConfig,
    /// Package configuration.
    pub packages: BTreeMap<String, PackageConfig>,
    /// Repository-local ecosystem plugin registrations.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugins: BTreeMap<EcosystemId, PluginConfig>,
    /// Resolver configuration.
    pub resolver: BTreeMap<EcosystemId, ResolverConfig>,
}

impl Config {
    pub fn plugin_definitions(&self) -> Result<Vec<PluginDefinition>, ConfigValidationError> {
        self.plugins
            .iter()
            .map(|(ecosystem, plugin)| plugin.definition(ecosystem))
            .collect()
    }

    fn validate(&self) -> Result<(), ConfigValidationError> {
        self.plugin_definitions()?;
        for (package, config) in &self.packages {
            if !config.resolver.is_builtin() && !self.plugins.contains_key(&config.resolver) {
                return Err(ConfigValidationError::PackagePluginNotRegistered {
                    package: PackageId::new(package),
                    ecosystem: config.resolver.clone(),
                });
            }
        }
        for ecosystem in self.resolver.keys() {
            if !ecosystem.is_builtin() && !self.plugins.contains_key(ecosystem) {
                return Err(ConfigValidationError::ResolverPluginNotRegistered {
                    ecosystem: ecosystem.clone(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("plugin path for {ecosystem} is not UTF-8: {path:?}")]
    NonUtf8PluginPath {
        ecosystem: EcosystemId,
        path: PathBuf,
    },
    #[error("invalid plugin definition for {ecosystem}: {source}")]
    PluginDefinition {
        ecosystem: EcosystemId,
        #[source]
        source: PluginRegistryError,
    },
    #[error("package {package} references unregistered plugin ecosystem {ecosystem}")]
    PackagePluginNotRegistered {
        package: PackageId,
        ecosystem: EcosystemId,
    },
    #[error("resolver configuration references unregistered plugin ecosystem {ecosystem}")]
    ResolverPluginNotRegistered { ecosystem: EcosystemId },
}

fn is_default_changelog_config(config: &ChangelogConfig) -> bool {
    config.template.is_none() && config.changeset_template.is_none()
}

fn is_default_release_config(config: &ReleaseConfig) -> bool {
    config.commit_message.is_none() && config.pull_request_title.is_none()
}

pub fn get_config_path(changeset_path: &Path) -> Result<PathBuf, ResolveError> {
    let config_path = changeset_path.join("config.toml");
    if !config_path.is_file() {
        let json_path = changeset_path.join("config.json");
        if json_path.is_file() {
            return Err(ResolveError::UnsupportedConfigFormat { path: json_path });
        }
        return Err(ResolveError::FileOrDirNotFound {
            path: "config.toml".into(),
        });
    }

    log::debug!("Found config path: {config_path:?}");

    Ok(config_path)
}

pub fn load_config(config_path: &Path) -> Result<Config, ResolveError> {
    let config_content = std::fs::read_to_string(config_path)?;
    load_config_from_str(config_path, &config_content)
}

pub fn load_config_from_str(
    config_path: &Path,
    config_content: &str,
) -> Result<Config, ResolveError> {
    if config_path
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("toml")
    {
        return Err(ResolveError::UnsupportedConfigFormat {
            path: config_path.to_path_buf(),
        });
    }
    let config: Config =
        toml_edit::de::from_str(config_content).map_err(|e| ResolveError::InvalidConfig {
            path: config_path.to_path_buf(),
            reason: e.to_string(),
        })?;
    config
        .validate()
        .map_err(|source| ResolveError::InvalidConfig {
            path: config_path.to_path_buf(),
            reason: source.to_string(),
        })?;
    Ok(config)
}

pub fn get_config() -> Result<Config, ResolveError> {
    let changeset_path = resolver::get_changeset_path()?;
    let config_path = get_config_path(&changeset_path)?;
    load_config(&config_path)
}

pub fn save_config(config_path: &Path, config: &Config) -> Result<(), ResolveError> {
    if config_path
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("toml")
    {
        return Err(ResolveError::UnsupportedConfigFormat {
            path: config_path.to_path_buf(),
        });
    }
    let config_content =
        toml_edit::ser::to_string_pretty(config).map_err(|e| ResolveError::InvalidConfig {
            path: config_path.to_path_buf(),
            reason: e.to_string(),
        })?;
    std::fs::write(config_path, config_content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::Path};

    use semifold_core::PackageId;

    use super::{
        ChangelogConfig, ChannelBump, CommandConfig, Config, PackageConfig, PreCheckConfig,
        ReleaseChannel, ReleaseConfig, load_config_from_str,
    };
    use crate::error::ResolveError;

    #[test]
    fn changelog_templates_round_trip_with_kebab_case_fields() {
        let config: ChangelogConfig = toml_edit::de::from_str(
            r#"
template = "Release {{ package.next_version }}"
changeset-template = "Change {{ changeset.summary }}"
"#,
        )
        .unwrap();
        assert_eq!(
            config.template.as_deref(),
            Some("Release {{ package.next_version }}")
        );
        assert_eq!(
            config.changeset_template.as_deref(),
            Some("Change {{ changeset.summary }}")
        );

        let rendered = toml_edit::ser::to_string(&config).unwrap();
        assert!(rendered.contains("changeset-template"));
        assert!(!rendered.contains("changeset_template"));
    }

    #[test]
    fn release_templates_round_trip_with_kebab_case_fields() {
        let config: ReleaseConfig = toml_edit::de::from_str(
            r#"
commit-message = "release {{ release.plan.fingerprint }}"
pull-request-title = "Release {{ release.plan.common_version }}"
"#,
        )
        .unwrap();
        assert_eq!(
            config.commit_message.as_deref(),
            Some("release {{ release.plan.fingerprint }}")
        );
        assert_eq!(
            config.pull_request_title.as_deref(),
            Some("Release {{ release.plan.common_version }}")
        );

        let rendered = toml_edit::ser::to_string(&config).unwrap();
        assert!(rendered.contains("commit-message"));
        assert!(rendered.contains("pull-request-title"));
        assert!(!rendered.contains("commit_message"));
        assert!(!rendered.contains("pull_request_title"));
    }

    #[test]
    fn semifold_json_configuration_is_rejected() {
        assert!(matches!(
            load_config_from_str(Path::new("config.json"), "{}"),
            Err(ResolveError::UnsupportedConfigFormat { .. })
        ));
    }

    #[test]
    fn pre_check_requires_an_explicit_type() {
        let http: PreCheckConfig =
            toml_edit::de::from_str("type = \"http\"\nurl = \"https://registry.test/pkg/1.0.0\"\n")
                .unwrap();
        assert!(matches!(
            http,
            PreCheckConfig::Http { retry, .. } if retry.is_empty()
        ));
        let http: PreCheckConfig = toml_edit::de::from_str(
            "type = \"http\"\nurl = \"https://registry.test/pkg/1.0.0\"\nretry = [2, 5, 15, 30]\n",
        )
        .unwrap();
        assert!(matches!(
            http,
            PreCheckConfig::Http { retry, .. } if retry == [2, 5, 15, 30]
        ));
        assert!(
            toml_edit::de::from_str::<PreCheckConfig>("url = \"https://registry.test\"").is_err()
        );
    }

    #[test]
    fn missing_and_explicit_stable_channels_are_equivalent() {
        let missing: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "rust"
"#,
        )
        .unwrap();
        let explicit: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "rust"
channel = "stable"
"#,
        )
        .unwrap();

        assert_eq!(missing.channel, ReleaseChannel::Stable);
        assert_eq!(missing.channel, explicit.channel);
    }

    #[test]
    fn named_channel_is_loaded_and_legacy_version_mode_is_migrated() {
        let named: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "rust"
channel = "alpha"
"#,
        )
        .unwrap();
        let legacy: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "rust"
version-mode = { pre-release = { tag = "beta" } }
"#,
        )
        .unwrap();

        assert_eq!(named.channel, ReleaseChannel::Named("alpha".to_string()));
        assert_eq!(legacy.channel, ReleaseChannel::Named("beta".to_string()));
    }

    #[test]
    fn loads_one_shot_channel_bump() {
        let package: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "rust"
channel = "alpha"
channel-bump = "preserve"
"#,
        )
        .unwrap();

        assert_eq!(package.channel_bump, Some(ChannelBump::Preserve));
    }

    #[test]
    fn configured_dependencies_use_stable_package_ids_and_round_trip() {
        let config: PackageConfig = toml_edit::de::from_str(
            r#"
path = "packages/node"
resolver = "nodejs"
depends-on = ["rust-core", "native-runtime"]
"#,
        )
        .unwrap();

        assert_eq!(
            config.depends_on,
            [
                PackageId::new("rust-core"),
                PackageId::new("native-runtime")
            ]
        );
        let rendered = toml_edit::ser::to_string(&config).unwrap();
        assert!(rendered.contains("depends-on = [\"rust-core\", \"native-runtime\"]"));
    }

    #[test]
    fn github_release_policy_defaults_to_publishability_and_allows_overrides() {
        let default: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "rust"
"#,
        )
        .unwrap();
        assert!(default.github_release_enabled(true));
        assert!(!default.github_release_enabled(false));

        let enabled: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "rust"
github-release = true
"#,
        )
        .unwrap();
        assert!(enabled.github_release_enabled(false));

        let disabled: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "rust"
github-release = false
"#,
        )
        .unwrap();
        assert!(!disabled.github_release_enabled(true));

        let rendered = toml_edit::ser::to_string(&enabled).unwrap();
        assert!(rendered.contains("github-release = true"));
        assert!(!rendered.contains("github_release"));
    }

    #[test]
    fn publish_policy_is_optional_and_round_trips_explicit_overrides() {
        let default: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "python"
"#,
        )
        .unwrap();
        assert_eq!(default.publish, None);
        let rendered = toml_edit::ser::to_string(&default).unwrap();
        assert!(!rendered.contains("publish ="));

        let enabled: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "rust"
publish = true
"#,
        )
        .unwrap();
        assert_eq!(enabled.publish, Some(true));
        assert!(
            toml_edit::ser::to_string(&enabled)
                .unwrap()
                .contains("publish = true")
        );

        let disabled: PackageConfig = toml_edit::de::from_str(
            r#"
path = "."
resolver = "python"
publish = false
"#,
        )
        .unwrap();
        assert_eq!(disabled.publish, Some(false));
        assert!(
            toml_edit::ser::to_string(&disabled)
                .unwrap()
                .contains("publish = false")
        );
    }

    #[test]
    fn command_fields_use_kebab_case_without_snake_case_aliases() {
        let command: CommandConfig = toml_edit::de::from_str(
            r#"
command = "cargo"
dry-run = true
extra-env = { RELEASE = "1" }
"#,
        )
        .unwrap();
        assert_eq!(command.dry_run, Some(true));
        assert_eq!(
            command.extra_env,
            BTreeMap::from([("RELEASE".to_string(), "1".to_string())])
        );

        let snake_case: CommandConfig = toml_edit::de::from_str(
            r#"
command = "cargo"
dry_run = true
extra_env = { RELEASE = "1" }
"#,
        )
        .unwrap();
        assert_eq!(snake_case.dry_run, None);
        assert!(snake_case.extra_env.is_empty());

        let rendered = toml_edit::ser::to_string(&command).unwrap();
        assert!(rendered.contains("dry-run = true"));
        assert!(rendered.contains("extra-env"));
        assert!(!rendered.contains("dry_run"));
        assert!(!rendered.contains("extra_env"));
    }

    #[test]
    fn loads_dynamic_plugin_configuration_with_an_optional_content_digest() {
        let config: Config = load_config_from_str(
            Path::new("config.toml"),
            r#"
[branches]
base = "main"
release = "release"

[tags]

[plugins."com.example.game"]
path = "plugins/game.js"
allowed-origins = ["https://API.example.com:443/"]

[packages.game]
path = "game"
resolver = "com.example.game"

[resolver."com.example.game"]
"#,
        )
        .unwrap();

        let ecosystem = semifold_core::EcosystemId::new("com.example.game").unwrap();
        assert_eq!(config.packages["game"].resolver, ecosystem);
        let definitions = config.plugin_definitions().unwrap();
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].sha256(), None);
        assert_eq!(definitions[0].path().as_str(), "plugins/game.js");
        assert_eq!(
            definitions[0]
                .allowed_origins()
                .iter()
                .map(super::PluginHttpOrigin::as_str)
                .collect::<Vec<_>>(),
            ["https://api.example.com"]
        );

        let rendered = toml_edit::ser::to_string_pretty(&config).unwrap();
        assert!(rendered.contains("[plugins.\"com.example.game\"]"));
        assert!(!rendered.contains("sha256"));
    }

    #[test]
    fn maps_a_configured_plugin_digest_to_a_strict_content_pin() {
        let config = load_config_from_str(
            Path::new("config.toml"),
            &format!(
                r#"
[branches]
base = "main"
release = "release"
[tags]
[plugins."com.example.game"]
path = "plugins/game.js"
sha256 = "{}"
[packages]
[resolver]
"#,
                "0".repeat(64)
            ),
        )
        .unwrap();

        let definitions = config.plugin_definitions().unwrap();
        assert_eq!(definitions[0].sha256(), Some("0".repeat(64).as_str()));
    }

    #[test]
    fn rejects_invalid_or_unregistered_dynamic_plugin_configuration() {
        let unregistered = r#"
[branches]
base = "main"
release = "release"
[tags]
[packages.game]
path = "game"
resolver = "com.example.game"
[resolver]
"#;
        assert!(matches!(
            load_config_from_str(Path::new("config.toml"), unregistered),
            Err(ResolveError::InvalidConfig { reason, .. })
                if reason.contains("unregistered plugin ecosystem")
        ));

        let invalid_digest = r#"
[branches]
base = "main"
release = "release"
[tags]
[plugins."com.example.game"]
path = "plugins/game.js"
sha256 = "invalid"
[packages]
[resolver]
"#;
        assert!(matches!(
            load_config_from_str(Path::new("config.toml"), invalid_digest),
            Err(ResolveError::InvalidConfig { reason, .. }) if reason.contains("SHA-256")
        ));

        let invalid_origin = invalid_digest.replace(
            "sha256 = \"invalid\"",
            "allowed-origins = [\"http://localhost\"]",
        );
        assert!(matches!(
            load_config_from_str(Path::new("config.toml"), &invalid_origin),
            Err(ResolveError::InvalidConfig { .. })
        ));
    }
}
