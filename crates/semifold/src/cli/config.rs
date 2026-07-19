use std::{fs::OpenOptions, io::Write, path::Path};

use anyhow::{Context as _, anyhow};
use camino::Utf8Path;
use clap::{Parser, Subcommand};
use rust_i18n::t;
use semifold_core::ConfigSyncWarning;
use semifold_resolver::{
    config,
    context::Context,
    resolver::{self, ResolverType},
};

use crate::{
    config_editor::TomlConfigEditor,
    config_sync::{ConfigSyncPlanningError, config_sync_scope, plan_config_sync},
};

#[derive(Parser, Debug)]
pub(crate) struct Config {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = t!("cli.config.commands.sync"))]
    Sync(Sync),
    #[command(about = t!("cli.config.commands.migrate"))]
    Migrate(Migrate),
    #[command(about = t!("cli.config.commands.channel"))]
    Channel(Channel),
}

#[derive(Parser, Debug)]
struct Sync {
    #[arg(long, help = t!("cli.config.flags.check_sync"))]
    check: bool,
    #[arg(
        long,
        conflicts_with = "check",
        help = t!("cli.config.flags.prune_sync")
    )]
    prune: bool,
    #[arg(
        long = "resolver",
        value_enum,
        help = t!("cli.config.flags.resolver_sync")
    )]
    resolvers: Vec<ResolverType>,
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
        Commands::Sync(options) => sync(options, ctx),
        Commands::Migrate(options) => migrate(options, ctx),
        Commands::Channel(channel) => manage_channel(channel, ctx),
    }
}

fn sync(options: &Sync, ctx: &Context) -> anyhow::Result<()> {
    let path = toml_config_path(ctx, t!("cli.config.command_sync").as_ref())?;
    let project_root = ctx
        .repo_root
        .as_deref()
        .context(t!("cli.config.sync_repo_root_not_found"))?;
    let config = ctx.config.as_ref().context(t!("cli.config.not_found"))?;
    let scope = config_sync_scope(config, &options.resolvers).map_err(|error| match error {
        ConfigSyncPlanningError::ResolverNotEnabled { resolver } => anyhow!(t!(
            "cli.config.sync_resolver_not_enabled",
            resolver = resolver.to_string()
        )),
        error => anyhow!(t!("cli.config.sync_planning_failed", error = error)),
    })?;
    if options.prune && !scope.is_complete {
        anyhow::bail!(t!("cli.config.sync_prune_partial_scan"));
    }
    let changesets = resolver::get_changesets(ctx)
        .map_err(|error| anyhow!(t!("cli.config.sync_planning_failed", error = error)))?;
    let plan = plan_config_sync(project_root, path, config, &changesets, &scope)
        .map_err(|error| anyhow!(t!("cli.config.sync_planning_failed", error = error)))?;

    report_sync_warnings(&plan.warnings);
    if !plan.missing.is_empty() {
        println!(
            "{}",
            t!(
                "cli.config.sync_missing",
                packages = plan
                    .missing
                    .iter()
                    .map(|package| package.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        );
    }
    if options.check {
        if plan.has_drift() {
            anyhow::bail!(t!("cli.config.sync_check_failed"));
        }
        println!("{}", t!("cli.config.sync_check_passed"));
        return Ok(());
    }
    if ctx.dry_run {
        println!("{}", t!("cli.config.sync_dry_run"));
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }
    if !plan.conflicts.is_empty() {
        anyhow::bail!(
            "{}",
            t!(
                "cli.config.sync_conflicts",
                count = plan.conflicts.len().to_string()
            )
        );
    }
    if !plan.has_drift() {
        println!("{}", t!("cli.config.sync_not_required"));
        return Ok(());
    }

    let path = Utf8Path::from_path(path).context(t!("cli.config.sync_non_utf8_path"))?;
    let mut editor = TomlConfigEditor::load(path)
        .map_err(|error| anyhow!(t!("cli.config.sync_edit_failed", error = error)))?;
    let original = editor.render();
    editor
        .apply(&plan, options.prune)
        .map_err(|error| anyhow!(t!("cli.config.sync_edit_failed", error = error)))?;
    let content = editor.render();
    if content == original {
        println!("{}", t!("cli.config.sync_no_safe_changes"));
        return Ok(());
    }

    write_atomically(path.as_std_path(), &content)?;
    println!("{}", t!("cli.config.synced", path = path));
    Ok(())
}

fn report_sync_warnings(warnings: &[ConfigSyncWarning]) {
    for warning in warnings {
        match warning {
            ConfigSyncWarning::ChangesetReferencesRenamedPackage {
                changeset,
                from,
                to,
            } => println!(
                "{}",
                t!(
                    "cli.config.sync_renamed_changeset_warning",
                    changeset = changeset.as_str(),
                    from = from.as_str(),
                    to = to.as_str()
                )
            ),
        }
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

    use clap::Parser as _;
    use semifold_resolver::{config, context::Context, resolver::ResolverType};

    use super::{
        ChannelTarget, Config as ConfigCommand, Migrate, Sync, migrate, plan_channel_update,
        plan_migration, sync, update_channel,
    };

    fn temporary_config_path(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "semifold-config-migrate-{name}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory.join("config.toml")
    }

    fn temporary_sync_root(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "semifold-config-sync-{name}-{}",
            std::process::id()
        ));
        fs::create_dir_all(directory.join("crates/app")).unwrap();
        fs::create_dir_all(directory.join(".changes")).unwrap();
        fs::write(
            directory.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/app\"]\n",
        )
        .unwrap();
        fs::write(
            directory.join("crates/app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        fs::write(
            directory.join(".changes/config.toml"),
            "[branches]\nbase = \"main\"\nrelease = \"release\"\n\n[tags]\n\n[packages]\n\n[resolver.rust.pre-check]\nurl = \"\"\n",
        )
        .unwrap();
        directory
    }

    fn sync_context(root: &std::path::Path, dry_run: bool) -> Context {
        let changeset_root = root.join(".changes");
        let config_path = changeset_root.join("config.toml");
        Context {
            config: Some(config::load_config(&config_path).unwrap()),
            changeset_root: Some(changeset_root),
            config_path: Some(config_path),
            repo_root: Some(root.to_path_buf()),
            dry_run,
            ..Default::default()
        }
    }

    fn enable_node_resolver(config_path: &std::path::Path) {
        fs::write(
            config_path,
            format!(
                "{}\n[resolver.nodejs.pre-check]\nurl = \"\"\n",
                fs::read_to_string(config_path).unwrap()
            ),
        )
        .unwrap();
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

    #[test]
    fn sync_adds_discovered_packages_and_is_idempotent() {
        let root = temporary_sync_root("write");
        let context = sync_context(&root, false);
        let config_path = root.join(".changes/config.toml");

        sync(
            &Sync {
                check: false,
                prune: false,
                resolvers: vec![],
            },
            &context,
        )
        .unwrap();
        let first = fs::read_to_string(&config_path).unwrap();
        assert!(first.contains("[packages.app]\npath = \"crates/app\"\nresolver = \"rust\""));

        sync(
            &Sync {
                check: false,
                prune: false,
                resolvers: vec![],
            },
            &sync_context(&root, false),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), first);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_dry_run_does_not_write_the_configuration() {
        let root = temporary_sync_root("dry-run");
        let context = sync_context(&root, true);
        let config_path = root.join(".changes/config.toml");
        let original = fs::read_to_string(&config_path).unwrap();

        sync(
            &Sync {
                check: false,
                prune: true,
                resolvers: vec![],
            },
            &context,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_check_reports_drift_without_writing_the_configuration() {
        let root = temporary_sync_root("check");
        let context = sync_context(&root, false);
        let config_path = root.join(".changes/config.toml");
        let original = fs::read_to_string(&config_path).unwrap();

        let error = sync(
            &Sync {
                check: true,
                prune: false,
                resolvers: vec![],
            },
            &context,
        )
        .unwrap_err();
        assert!(error.to_string().contains("out of sync"));
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_check_succeeds_when_the_configuration_is_current() {
        let root = temporary_sync_root("check-current");
        let config_path = root.join(".changes/config.toml");
        sync(
            &Sync {
                check: false,
                prune: false,
                resolvers: vec![],
            },
            &sync_context(&root, false),
        )
        .unwrap();

        sync(
            &Sync {
                check: true,
                prune: false,
                resolvers: vec![],
            },
            &sync_context(&root, false),
        )
        .unwrap();
        assert!(
            fs::read_to_string(&config_path)
                .unwrap()
                .contains("[packages.app]")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_prune_removes_packages_missing_from_the_complete_scan() {
        let root = temporary_sync_root("prune");
        let config_path = root.join(".changes/config.toml");
        fs::write(
            &config_path,
            format!(
                "{}\n[packages.removed]\n# removed package fields must leave with its table\npath = \"crates/removed\"\nresolver = \"rust\"\ncustom = \"value\"\n",
                fs::read_to_string(&config_path).unwrap()
            ),
        )
        .unwrap();

        sync(
            &Sync {
                check: false,
                prune: true,
                resolvers: vec![],
            },
            &sync_context(&root, false),
        )
        .unwrap();
        let synced = fs::read_to_string(&config_path).unwrap();
        assert!(synced.contains("[packages.app]"));
        assert!(!synced.contains("[packages.removed]"));
        assert!(!synced.contains("custom = \"value\""));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_accepts_repeated_resolver_flags() {
        assert!(
            ConfigCommand::try_parse_from([
                "config",
                "sync",
                "--resolver",
                "rust",
                "--resolver",
                "rust",
            ])
            .is_ok()
        );
    }

    #[test]
    fn sync_limits_planning_to_selected_resolvers() {
        let root = temporary_sync_root("scope");
        let config_path = root.join(".changes/config.toml");
        enable_node_resolver(&config_path);
        fs::write(
            &config_path,
            format!(
                "{}\n[packages.node-only]\npath = \"packages/node-only\"\nresolver = \"nodejs\"\n",
                fs::read_to_string(&config_path).unwrap()
            ),
        )
        .unwrap();
        let rust_only = Sync {
            check: false,
            prune: false,
            resolvers: vec![ResolverType::Rust],
        };

        sync(&rust_only, &sync_context(&root, false)).unwrap();
        let synced = fs::read_to_string(&config_path).unwrap();
        assert!(synced.contains("[packages.app]"));
        assert!(synced.contains("[packages.node-only]"));

        sync(
            &Sync {
                check: true,
                prune: false,
                resolvers: vec![ResolverType::Rust],
            },
            &sync_context(&root, false),
        )
        .unwrap();

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_rejects_unenabled_resolvers_without_writing() {
        let root = temporary_sync_root("unconfigured-resolver");
        let config_path = root.join(".changes/config.toml");
        let original = fs::read_to_string(&config_path).unwrap();

        let error = sync(
            &Sync {
                check: false,
                prune: false,
                resolvers: vec![ResolverType::Nodejs],
            },
            &sync_context(&root, false),
        )
        .unwrap_err();
        assert!(error.to_string().contains("not enabled"));
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_rejects_prune_for_partial_resolver_scopes_without_writing() {
        let root = temporary_sync_root("partial-prune");
        let config_path = root.join(".changes/config.toml");
        enable_node_resolver(&config_path);
        let original = fs::read_to_string(&config_path).unwrap();

        let error = sync(
            &Sync {
                check: false,
                prune: true,
                resolvers: vec![ResolverType::Rust],
            },
            &sync_context(&root, false),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires scanning every enabled resolver")
        );
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);

        fs::remove_dir_all(root).unwrap();
    }
}
