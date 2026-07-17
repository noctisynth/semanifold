use std::{fs::OpenOptions, io::Write, path::Path};

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use rust_i18n::t;
use semifold_resolver::{config, context::Context};

#[derive(Parser, Debug)]
pub(crate) struct Config {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = t!("cli.config.commands.migrate"))]
    Migrate(Migrate),
    #[command(about = t!("cli.config.commands.channel"))]
    Channel(Channel),
}

#[derive(Parser, Debug)]
struct Migrate {
    #[arg(long, help = t!("cli.config.flags.check_migration"))]
    check: bool,
}

#[derive(Parser, Debug)]
struct Channel {
    #[command(subcommand)]
    command: ChannelCommands,
}

#[derive(Subcommand, Debug)]
enum ChannelCommands {
    #[command(about = t!("cli.config.commands.channel_set"))]
    Set(ChannelSet),
    #[command(about = t!("cli.config.commands.channel_clear"))]
    Clear(ChannelClear),
}

#[derive(Parser, Debug)]
struct ChannelSet {
    #[arg(help = t!("cli.config.flags.channel"))]
    channel: String,
    #[command(flatten)]
    target: ChannelTarget,
}

#[derive(Parser, Debug)]
struct ChannelClear {
    #[command(flatten)]
    target: ChannelTarget,
}

#[derive(clap::Args, Debug)]
struct ChannelTarget {
    #[arg(
        long = "package",
        required_unless_present = "all",
        conflicts_with = "all",
        help = t!("cli.config.flags.package")
    )]
    packages: Vec<String>,
    #[arg(long, conflicts_with = "packages", help = t!("cli.config.flags.all"))]
    all: bool,
    #[arg(long, help = t!("cli.config.flags.check_channel"))]
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
        Commands::Channel(channel) => manage_channel(channel, ctx),
    }
}

fn manage_channel(command: &Channel, ctx: &Context) -> anyhow::Result<()> {
    match &command.command {
        ChannelCommands::Set(options) => {
            if options.channel.trim().is_empty() || options.channel == "stable" {
                anyhow::bail!(t!("cli.config.channel_set_requires_named"));
            }
            update_channel(Some(&options.channel), &options.target, ctx)
        }
        ChannelCommands::Clear(options) => update_channel(None, &options.target, ctx),
    }
}

fn update_channel(
    channel: Option<&str>,
    target: &ChannelTarget,
    ctx: &Context,
) -> anyhow::Result<()> {
    let path = toml_config_path(ctx, t!("cli.config.command_channel").as_ref())?;
    let original = std::fs::read_to_string(path)?;
    config::load_config(path)?;
    let plan = plan_channel_update(&original, channel, &target.packages, target.all)?;
    if plan.packages.is_empty() {
        println!("{}", t!("cli.config.channels_already_match"));
        return Ok(());
    }

    let requested = channel.unwrap_or("stable");
    println!(
        "{}",
        t!(
            "cli.config.updating_channel",
            channel = requested,
            packages = plan.packages.join(", ")
        )
    );
    if target.check {
        anyhow::bail!(t!("cli.config.channels_mismatch"));
    }
    if ctx.dry_run {
        return Ok(());
    }

    config::load_config_from_str(path, &plan.content)?;
    write_atomically(path, &plan.content)?;
    println!("{}", t!("cli.config.updated", path = path.display()));
    Ok(())
}

fn migrate(options: &Migrate, ctx: &Context) -> anyhow::Result<()> {
    let path = toml_config_path(ctx, t!("cli.config.command_migrate").as_ref())?;

    let original = std::fs::read_to_string(path)?;
    config::load_config(path)?;
    let plan = plan_migration(&original)?;
    if plan.packages.is_empty() {
        println!("{}", t!("cli.config.migration_not_required"));
        return Ok(());
    }

    println!(
        "{}",
        t!(
            "cli.config.migration_required",
            packages = plan.packages.join(", ")
        )
    );
    if options.check {
        anyhow::bail!(t!("cli.config.migration_required_error"));
    }
    if ctx.dry_run {
        return Ok(());
    }

    config::load_config_from_str(path, &plan.content)?;
    write_atomically(path, &plan.content)?;
    println!("{}", t!("cli.config.migrated", path = path.display()));
    Ok(())
}

fn toml_config_path<'a>(ctx: &'a Context, command: &str) -> anyhow::Result<&'a Path> {
    let path = ctx
        .config_path
        .as_deref()
        .context(t!("cli.config.not_found"))?;
    if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
        anyhow::bail!(t!("cli.config.unsupported_format", command = command));
    }
    Ok(path)
}

fn plan_migration(content: &str) -> anyhow::Result<MigrationPlan> {
    let mut document = content.parse::<toml_edit::DocumentMut>()?;
    let packages = document["packages"]
        .as_table_mut()
        .context(t!("cli.config.missing_packages_table"))?;
    let mut migrated = Vec::new();

    for (name, package) in packages.iter_mut() {
        let table = package
            .as_table_like_mut()
            .with_context(|| t!("cli.config.package_must_be_table", package = name))?;
        let has_channel = table.contains_key("channel");
        let legacy = table.get("version-mode").map(ToString::to_string);
        let Some(legacy) = legacy else {
            continue;
        };
        if has_channel {
            anyhow::bail!(t!("cli.config.channel_legacy_conflict", package = name));
        }

        let version_mode = parse_legacy_version_mode(&legacy)
            .with_context(|| t!("cli.config.invalid_legacy_version_mode", package = name))?;
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

fn plan_channel_update(
    content: &str,
    channel: Option<&str>,
    requested: &[String],
    all: bool,
) -> anyhow::Result<MigrationPlan> {
    let mut document = content.parse::<toml_edit::DocumentMut>()?;
    let packages = document["packages"]
        .as_table_mut()
        .context(t!("cli.config.missing_packages_table"))?;
    let targets = if all {
        packages.iter().map(|(name, _)| name.to_string()).collect()
    } else {
        requested.to_vec()
    };

    for name in &targets {
        if !packages.contains_key(name) {
            anyhow::bail!(t!("cli.config.package_not_configured", package = name));
        }
    }

    let mut updated = Vec::new();
    for name in targets {
        let table = packages[&name]
            .as_table_like_mut()
            .with_context(|| t!("cli.config.package_must_be_table", package = name))?;
        let current = table
            .get("channel")
            .and_then(toml_edit::Item::as_value)
            .and_then(toml_edit::Value::as_str);
        if current == channel {
            continue;
        }
        match channel {
            Some(channel) => {
                table.insert("channel", toml_edit::value(channel));
            }
            None => {
                table.remove("channel");
            }
        }
        updated.push(name.to_string());
    }

    Ok(MigrationPlan {
        content: document.to_string(),
        packages: updated,
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
        .with_context(|| {
            t!(
                "cli.config.create_temporary_failed",
                path = temporary.display()
            )
        })?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use semifold_resolver::context::Context;

    use super::{
        ChannelTarget, Migrate, migrate, plan_channel_update, plan_migration, update_channel,
    };

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

    #[test]
    fn sets_and_clears_only_targeted_package_channels_idempotently() {
        let content = r#"
[packages.app]
# retained
path = "."
resolver = "rust"
assets = ["dist/*"]

[packages.library]
path = "library"
resolver = "rust"
channel = "beta"
"#;
        let requested = vec!["app".to_string()];
        let set = plan_channel_update(content, Some("alpha"), &requested, false).unwrap();

        assert_eq!(set.packages, ["app"]);
        assert!(set.content.contains("# retained"));
        assert!(set.content.contains("assets = [\"dist/*\"]"));
        assert!(set.content.contains("channel = \"alpha\""));
        assert!(set.content.contains("[packages.library]"));
        assert!(set.content.contains("channel = \"beta\""));
        assert!(
            plan_channel_update(&set.content, Some("alpha"), &requested, false)
                .unwrap()
                .packages
                .is_empty()
        );

        let clear = plan_channel_update(&set.content, None, &requested, false).unwrap();
        assert_eq!(clear.packages, ["app"]);
        assert!(!clear.content.contains("channel = \"alpha\""));
        assert!(clear.content.contains("channel = \"beta\""));
    }

    #[test]
    fn sets_channel_for_all_configured_packages() {
        let plan = plan_channel_update(
            r#"
[packages.app]
path = "."
resolver = "rust"

[packages.library]
path = "library"
resolver = "rust"
"#,
            Some("alpha"),
            &[],
            true,
        )
        .unwrap();

        assert_eq!(plan.packages, ["app", "library"]);
        assert_eq!(plan.content.matches("channel = \"alpha\"").count(), 2);
    }

    #[test]
    fn channel_check_does_not_write_when_state_differs() {
        let path = temporary_config_path("channel-check");
        let original = r#"
[branches]
base = "main"
release = "release"

[packages.app]
path = "."
resolver = "rust"

[tags]

[resolver]
"#;
        fs::write(&path, original).unwrap();
        let context = Context {
            config_path: Some(path.clone()),
            ..Default::default()
        };
        let target = ChannelTarget {
            packages: vec!["app".to_string()],
            all: false,
            check: true,
        };

        let error = update_channel(Some("alpha"), &target, &context).unwrap_err();

        assert!(error.to_string().contains("do not match"));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
