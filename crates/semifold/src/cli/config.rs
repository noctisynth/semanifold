use std::{fs::OpenOptions, io::Write, path::Path};

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use semifold_resolver::{config, context::Context};

#[derive(Parser, Debug)]
pub(crate) struct Config {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Migrate legacy version-mode fields to release channels.
    Migrate(Migrate),
}

#[derive(Parser, Debug)]
struct Migrate {
    /// Report whether migration is required without writing the configuration.
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct MigrationPlan {
    content: String,
    packages: Vec<String>,
}

pub(crate) fn run(command: &Config, ctx: &Context) -> anyhow::Result<()> {
    match &command.command {
        Commands::Migrate(options) => migrate(options, ctx),
    }
}

fn migrate(options: &Migrate, ctx: &Context) -> anyhow::Result<()> {
    let path = ctx
        .config_path
        .as_deref()
        .context("Semifold configuration was not found")?;
    if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
        anyhow::bail!("config migrate supports only TOML configuration files");
    }

    let original = std::fs::read_to_string(path)?;
    config::load_config(path)?;
    let plan = plan_migration(&original)?;
    if plan.packages.is_empty() {
        println!("Configuration does not need migration.");
        return Ok(());
    }

    println!(
        "Configuration migration required for: {}",
        plan.packages.join(", ")
    );
    if options.check {
        anyhow::bail!("configuration migration is required");
    }
    if ctx.dry_run {
        return Ok(());
    }

    config::load_config_from_str(path, &plan.content)?;
    write_atomically(path, &plan.content)?;
    println!("Migrated {}.", path.display());
    Ok(())
}

fn plan_migration(content: &str) -> anyhow::Result<MigrationPlan> {
    let mut document = content.parse::<toml_edit::DocumentMut>()?;
    let packages = document["packages"]
        .as_table_mut()
        .context("configuration is missing the [packages] table")?;
    let mut migrated = Vec::new();

    for (name, package) in packages.iter_mut() {
        let table = package
            .as_table_like_mut()
            .with_context(|| format!("package {name} must be a table"))?;
        let has_channel = table.contains_key("channel");
        let legacy = table.get("version-mode").map(ToString::to_string);
        let Some(legacy) = legacy else {
            continue;
        };
        if has_channel {
            anyhow::bail!("package {name} contains both channel and version-mode");
        }

        let version_mode = parse_legacy_version_mode(&legacy)
            .with_context(|| format!("package {name} has an invalid version-mode"))?;
        table.remove("version-mode");
        if let semifold_resolver::config::VersionMode::PreRelease { tag } = version_mode {
            table.insert("channel", toml_edit::value(tag));
        }
        migrated.push(name.to_string());
    }

    Ok(MigrationPlan {
        content: document.to_string(),
        packages: migrated,
    })
}

fn parse_legacy_version_mode(
    value: &str,
) -> anyhow::Result<semifold_resolver::config::VersionMode> {
    #[derive(serde::Deserialize)]
    struct LegacyVersionMode {
        #[serde(rename = "version-mode")]
        version_mode: semifold_resolver::config::VersionMode,
    }

    Ok(
        toml_edit::de::from_str::<LegacyVersionMode>(&format!("version-mode = {value}"))?
            .version_mode,
    )
}

fn write_atomically(path: &Path, content: &str) -> anyhow::Result<()> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("tmp");
    let temporary = path.with_extension(format!("{extension}.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("failed to create temporary config {}", temporary.display()))?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use semifold_resolver::context::Context;

    use super::{Migrate, migrate, plan_migration};

    fn temporary_config_path(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "semifold-config-migrate-{name}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory.join("config.toml")
    }

    #[test]
    fn migrates_legacy_pre_release_without_touching_comments() {
        let plan = plan_migration(
            r#"
[packages.app]
# retained
path = "."
resolver = "rust"
version-mode = { pre-release = { tag = "alpha" } }
"#,
        )
        .unwrap();

        assert_eq!(plan.packages, ["app"]);
        assert!(plan.content.contains("# retained"));
        assert!(plan.content.contains("channel = \"alpha\""));
        assert!(!plan.content.contains("version-mode"));
        assert!(plan_migration(&plan.content).unwrap().packages.is_empty());
    }

    #[test]
    fn migrates_semantic_mode_to_an_implicit_stable_channel() {
        let plan = plan_migration(
            r#"
[packages.app]
path = "."
resolver = "rust"
version-mode = "semantic"
"#,
        )
        .unwrap();

        assert_eq!(plan.packages, ["app"]);
        assert!(!plan.content.contains("version-mode"));
        assert!(!plan.content.contains("channel"));
    }

    #[test]
    fn rejects_ambiguous_channel_and_legacy_mode() {
        let error = plan_migration(
            r#"
[packages.app]
path = "."
resolver = "rust"
channel = "alpha"
version-mode = "semantic"
"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("both channel and version-mode"));
    }

    #[test]
    fn check_reports_required_migration_without_writing() {
        let path = temporary_config_path("check");
        let original = r#"
[branches]
base = "main"
release = "release"

[packages.app]
path = "."
resolver = "rust"
version-mode = "semantic"

[tags]

[resolver]
"#;
        fs::write(&path, original).unwrap();
        let context = Context {
            config_path: Some(path.clone()),
            ..Default::default()
        };

        let error = migrate(&Migrate { check: true }, &context).unwrap_err();

        assert!(
            error.to_string().contains("migration is required"),
            "unexpected error: {error:#}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
