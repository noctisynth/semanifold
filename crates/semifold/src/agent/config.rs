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

#[derive(Debug, Deserialize)]
struct CargoManifest {
    package: CargoPackage,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    metadata: CargoMetadata,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    #[serde(rename = "semifold")]
    agent: SemifoldConfig,
}

#[derive(Debug, Deserialize)]
struct SemifoldConfig {
    base_url: String,
    model: String,
}

impl AgentConfig {
    pub fn load() -> anyhow::Result<Self> {
        load_dotenv();

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

fn load_dotenv() {
    if std::env::var("CARGO_MANIFEST_DIR").is_ok() {
        dotenvy::dotenv().ok();
        return;
    }

    if let Ok(exe_path) = std::env::current_exe() {
        let mut search_dir = exe_path.parent().map(|p| p.to_path_buf());

        for _ in 0..5 {
            if let Some(dir) = &search_dir {
                let env_path = dir.join(".env");
                if env_path.exists() {
                    load_env_file(&env_path);
                    return;
                }
                search_dir = dir.parent().map(|p| p.to_path_buf());
            }
        }
    }

    dotenvy::dotenv().ok();
}

fn load_env_file(path: &std::path::Path) {
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                if !value.is_empty() {
                    unsafe {
                        std::env::set_var(key, value);
                    }
                }
            }
        }
    }
}

fn load_provider_config() -> anyhow::Result<ProviderConfig> {
    let cargo_toml_path = find_cargo_toml()?;

    let content = std::fs::read_to_string(&cargo_toml_path)?;
    let manifest: CargoManifest = toml::from_str(&content)?;

    Ok(ProviderConfig {
        base_url: manifest.package.metadata.agent.base_url,
        model: manifest.package.metadata.agent.model,
    })
}

fn find_cargo_toml() -> anyhow::Result<PathBuf> {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = PathBuf::from(&manifest_dir).join("Cargo.toml");
        if path.exists() {
            return Ok(path);
        }
    }

    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let mut search_dir: Option<PathBuf> = Some(exe_dir.to_path_buf());

        for _ in 0..5 {
            if let Some(dir) = &search_dir {
                let crates_dir = dir.join("crates");
                if crates_dir.exists()
                    && let Ok(entries) = std::fs::read_dir(&crates_dir)
                {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let cargo_path = path.join("Cargo.toml");
                            if cargo_path.exists() {
                                let content = std::fs::read_to_string(&cargo_path)?;
                                if content.contains("package.metadata.semifold") {
                                    return Ok(cargo_path);
                                }
                            }
                        }
                    }
                }
                search_dir = dir.parent().map(|p| p.to_path_buf());
            }
        }
    }

    anyhow::bail!("Cannot find Cargo.toml with package.metadata.semifold")
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

    match std::fs::read_to_string(&config_path) {
        Ok(content) => content.contains("[provider]"),
        Err(_) => false,
    }
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
