pub mod client;
pub mod config;
pub mod git;
pub mod git_checker;
pub mod prompter;

use anyhow::Context;
use inquire::Select;
use semifold_resolver::context::Context as ResolverContext;

pub fn run(ctx: &ResolverContext) -> anyhow::Result<()> {
    let agent_config = config::AgentConfig::load()?;

    let diff = git::get_changeset_diff(ctx)?;

    let changeset_content = prompter::generate_changeset(&agent_config, &diff, ctx)?;

    let temp_path = ctx
        .changeset_root
        .as_ref()
        .context("changeset root not found")?
        .join(".temp_changeset.md");

    std::fs::write(&temp_path, &changeset_content)?;

    let confirmed = inquire::Confirm::new("Accept this changeset?")
        .with_default(true)
        .prompt()?;

    if !confirmed {
        std::fs::remove_file(&temp_path)?;
        anyhow::bail!("Changeset rejected");
    }

    let name = sanitize_filename(
        &inquire::Text::new("Enter changeset name:")
            .prompt()
            .unwrap_or_else(|_| "changeset".to_string()),
    );

    let final_path = ctx
        .changeset_root
        .as_ref()
        .context("changeset root not found")?
        .join(format!("{}.md", name));

    std::fs::rename(&temp_path, &final_path)?;

    println!("Changeset created: {}", final_path.display());

    Ok(())
}

pub fn setup_agent_config() -> anyhow::Result<()> {
    let provider_options: Vec<&str> = config::PROVIDER_PRESETS
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<Vec<_>>();

    let selected_provider = Select::new("Select provider", provider_options)
        .prompt()?;

    let (base_url, model) = config::get_preset(selected_provider)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", selected_provider))?;

    let api_key = inquire::Text::new("Enter API key")
        .prompt()?;

    let changeset_root = config::find_changeset_root()?;
    let config_path = changeset_root.join("config.toml");
    let env_path = std::env::current_dir()?.join(".env");

    let content = std::fs::read_to_string(&config_path)?;
    let mut toml_content: toml_edit::DocumentMut = content.parse()?;

    let mut provider_table = toml_edit::Table::new();
    provider_table.insert("base_url", toml_edit::value(base_url));
    provider_table.insert("model", toml_edit::value(model));

    toml_content.insert("provider", toml_edit::Item::Table(provider_table));

    std::fs::write(&config_path, toml_content.to_string())?;
    std::fs::write(&env_path, format!("AGENT_API_KEY={}\n", api_key))?;

    println!("Added [provider] section to .changes/config.toml");
    println!("Created .env file with AGENT_API_KEY");

    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    const ILLEGAL_CHARS: [char; 8] = ['<', '>', ':', '"', '/', '\\', '|', ' '];
    name.chars()
        .map(|c| {
            if ILLEGAL_CHARS.contains(&c) {
                '-'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}
