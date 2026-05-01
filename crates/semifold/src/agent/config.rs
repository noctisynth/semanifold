use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub api_type: ApiType,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiType {
    OpenAI,
    Anthropic,
}

impl AgentConfig {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let api_key = std::env::var("AGENT_API_KEY")
            .map_err(|_| anyhow::anyhow!("AGENT_API_KEY not set in .env file"))?;

        let provider_config = load_provider_config()?;

        let api_type = if provider_config.base_url.contains("anthropic") {
            ApiType::Anthropic
        } else {
            ApiType::OpenAI
        };

        Ok(AgentConfig {
            api_type,
            base_url: provider_config.base_url,
            api_key,
            model: provider_config.model,
        })
    }
}

fn load_provider_config() -> anyhow::Result<ProviderConfig> {
    let changeset_root = find_changeset_root()?;
    let config_path = changeset_root.join("config.toml");

    let content = std::fs::read_to_string(&config_path)?;
    let toml_content: toml_edit::DocumentMut = content.parse()?;

    let provider = toml_content
        .get("provider")
        .and_then(|p| p.as_table())
        .ok_or_else(|| anyhow::anyhow!("[provider] section not found in config.toml"))?;

    let base_url = provider
        .get("base_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("base_url not found in [provider]"))?
        .to_string();

    let model = provider
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("model not found in [provider]"))?
        .to_string();

    Ok(ProviderConfig { base_url, model })
}

pub fn find_changeset_root() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let changeset_root = cwd.join(".changes");
    if changeset_root.exists() {
        return Ok(changeset_root);
    }
    let changesets_root = cwd.join(".changesets");
    if changesets_root.exists() {
        return Ok(changesets_root);
    }
    anyhow::bail!("Neither .changes nor .changesets directory found")
}

pub fn has_provider_config() -> bool {
    let changeset_root = match find_changeset_root() {
        Ok(path) => path,
        Err(_) => return false,
    };
    let config_path = changeset_root.join("config.toml");

    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
            return doc.get("provider").is_some();
        }
    }
    false
}

pub const PROVIDER_PRESETS: &[(&str, &str, &str)] = &[
    ("minimax", "https://api.minimaxi.com", "MiniMax-M2.5"),
    ("deepseek", "https://api.deepseek.com", "deepseek-v4-pro"),
    ("anthropic", "https://api.anthropic.com", "claude-sonnet-4"),
];

pub fn get_preset(name: &str) -> Option<(&'static str, &'static str)> {
    PROVIDER_PRESETS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, base_url, model)| (*base_url, *model))
}
