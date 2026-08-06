use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use semifold_core::PackageId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{error::ResolveError, resolver};

#[derive(Serialize, Deserialize, Debug)]
pub struct BranchesConfig {
    pub base: String,
    pub release: String,
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
    pub resolver: resolver::ResolverType,
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
    resolver: resolver::ResolverType,
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

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    /// Branch configuration.
    pub branches: BranchesConfig,
    /// Tag configuration.
    pub tags: BTreeMap<String, String>,
    /// Changelog template configuration.
    #[serde(default, skip_serializing_if = "is_default_changelog_config")]
    pub changelog: ChangelogConfig,
    /// Package configuration.
    pub packages: BTreeMap<String, PackageConfig>,
    /// Resolver configuration.
    pub resolver: BTreeMap<resolver::ResolverType, ResolverConfig>,
}

fn is_default_changelog_config(config: &ChangelogConfig) -> bool {
    config.template.is_none() && config.changeset_template.is_none()
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
    let config =
        toml_edit::de::from_str(config_content).map_err(|e| ResolveError::InvalidConfig {
            path: config_path.to_path_buf(),
            reason: e.to_string(),
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
        ChangelogConfig, ChannelBump, CommandConfig, PackageConfig, PreCheckConfig, ReleaseChannel,
        load_config_from_str,
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
}
