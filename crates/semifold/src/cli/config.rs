use std::{collections::BTreeSet, fs::OpenOptions, io::Write, path::Path};

use anyhow::{Context as _, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use rust_i18n::t;
use semifold_core::ConfigSyncWarning;
use semifold_engine::{
    AppError, ConfigSyncOptions, Project, SemifoldService, SystemDependencies,
    config_sync::ConfigSyncPlanningError,
};
use semifold_resolver::{config, resolver::ResolverType};

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
    #[arg(long, value_enum, help = t!("cli.config.flags.bump"))]
    bump: Option<ChannelBumpArg>,
    #[command(flatten)]
    target: ChannelTarget,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ChannelBumpArg {
    Preserve,
    Patch,
    Minor,
    Major,
}

impl ChannelBumpArg {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::Patch => "patch",
            Self::Minor => "minor",
            Self::Major => "major",
        }
    }
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

pub(crate) fn run(command: &Config, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    match &command.command {
        Commands::Sync(options) => sync(options, project, dry_run),
        Commands::Migrate(options) => migrate(options, project, dry_run),
        Commands::Channel(channel) => manage_channel(channel, project, dry_run),
    }
}

fn sync(options: &Sync, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    toml_config_path(project)
        .map_err(|error| render_config_path_error(error, t!("cli.config.command_sync").as_ref()))?;
    let service = SemifoldService::new(SystemDependencies);
    let plan = service
        .plan_config_sync(
            project,
            &ConfigSyncOptions {
                resolvers: options.resolvers.clone(),
                prune_missing: options.prune,
            },
        )
        .map_err(render_config_sync_error)?;

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
    if dry_run {
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

    let report = service
        .apply_config_sync(&plan)
        .map_err(|error| anyhow!(t!("cli.config.sync_edit_failed", error = error)))?;
    if !report.changed {
        println!("{}", t!("cli.config.sync_no_safe_changes"));
        return Ok(());
    }
    println!("{}", t!("cli.config.synced", path = report.path));
    Ok(())
}

fn render_config_sync_error(error: AppError) -> anyhow::Error {
    match error {
        AppError::ConfigSyncPlanning(ConfigSyncPlanningError::ResolverNotEnabled { resolver }) => {
            anyhow!(t!(
                "cli.config.sync_resolver_not_enabled",
                resolver = resolver.to_string()
            ))
        }
        AppError::ConfigSyncPlanning(ConfigSyncPlanningError::IncompletePrune) => {
            anyhow!(t!("cli.config.sync_prune_partial_scan"))
        }
        error => anyhow!(t!("cli.config.sync_planning_failed", error = error)),
    }
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

fn manage_channel(command: &Channel, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    match &command.command {
        ChannelCommands::Set(options) => {
            if options.channel.trim().is_empty() || options.channel == "stable" {
                anyhow::bail!(t!("cli.config.channel_set_requires_named"));
            }
            update_channel(
                Some(&options.channel),
                options.bump,
                &options.target,
                project,
                dry_run,
            )
        }
        ChannelCommands::Clear(options) => {
            update_channel(None, None, &options.target, project, dry_run)
        }
    }
}

fn update_channel(
    channel: Option<&str>,
    bump: Option<ChannelBumpArg>,
    target: &ChannelTarget,
    project: &Project,
    dry_run: bool,
) -> anyhow::Result<()> {
    let path = toml_config_path(project).map_err(|error| {
        render_config_path_error(error, t!("cli.config.command_channel").as_ref())
    })?;
    let original = std::fs::read_to_string(path)?;
    config::load_config(path)?;
    let plan = plan_channel_update(&original, channel, bump, &target.packages, target.all)?;
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
    if dry_run {
        return Ok(());
    }

    config::load_config_from_str(path, &plan.content)?;
    write_atomically(path, &plan.content)?;
    println!("{}", t!("cli.config.updated", path = path.display()));
    Ok(())
}

fn migrate(options: &Migrate, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    let path = toml_config_path(project).map_err(|error| {
        render_config_path_error(error, t!("cli.config.command_migrate").as_ref())
    })?;

    let original = std::fs::read_to_string(path)?;
    let plan = plan_migration(&original)?;
    config::load_config_from_str(path, &plan.content)?;
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
    if dry_run {
        return Ok(());
    }

    write_atomically(path, &plan.content)?;
    println!("{}", t!("cli.config.migrated", path = path.display()));
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
enum ConfigPathError {
    UnsupportedConfigFormat,
}

fn toml_config_path(project: &Project) -> Result<&Path, ConfigPathError> {
    let path = project.config_path.as_std_path();
    if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
        return Err(ConfigPathError::UnsupportedConfigFormat);
    }
    Ok(path)
}

fn render_config_path_error(error: ConfigPathError, command: &str) -> anyhow::Error {
    match error {
        ConfigPathError::UnsupportedConfigFormat => {
            anyhow!(t!("cli.config.unsupported_format", command = command))
        }
    }
}

fn plan_migration(content: &str) -> anyhow::Result<MigrationPlan> {
    let mut document = content.parse::<toml_edit::DocumentMut>()?;
    let mut migrated = BTreeSet::new();
    migrate_snake_case_fields(document.as_table_mut(), &mut migrated)?;
    let packages = document
        .get_mut("packages")
        .and_then(toml_edit::Item::as_table_mut)
        .context(t!("cli.config.missing_packages_table"))?;

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
        migrated.insert(name.to_string());
    }

    Ok(MigrationPlan {
        content: document.to_string(),
        packages: migrated.into_iter().collect(),
    })
}

const SNAKE_CASE_FIELDS: [(&str, &str); 8] = [
    ("version_mode", "version-mode"),
    ("channel_bump", "channel-bump"),
    ("depends_on", "depends-on"),
    ("pre_check", "pre-check"),
    ("post_version", "post-version"),
    ("extra_headers", "extra-headers"),
    ("extra_env", "extra-env"),
    ("dry_run", "dry-run"),
];

fn migrate_snake_case_fields(
    document: &mut toml_edit::Table,
    migrated: &mut BTreeSet<String>,
) -> anyhow::Result<()> {
    if let Some(packages) = document
        .get_mut("packages")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        for (name, package) in packages.iter_mut() {
            if let Some(package) = package.as_table_like_mut() {
                rename_table_fields(
                    package,
                    &format!("packages.{name}"),
                    &SNAKE_CASE_FIELDS[..3],
                    migrated,
                )?;
            }
        }
    }

    if let Some(resolvers) = document
        .get_mut("resolver")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        for (name, resolver) in resolvers.iter_mut() {
            let Some(resolver) = resolver.as_table_like_mut() else {
                continue;
            };
            let scope = format!("resolver.{name}");
            rename_table_fields(resolver, &scope, &SNAKE_CASE_FIELDS[3..5], migrated)?;

            if let Some(pre_check) = resolver
                .get_mut("pre-check")
                .and_then(toml_edit::Item::as_table_like_mut)
            {
                rename_table_fields(
                    pre_check,
                    &format!("{scope}.pre-check"),
                    &SNAKE_CASE_FIELDS[5..6],
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
    commands: &mut toml_edit::Item,
    scope: &str,
    migrated: &mut BTreeSet<String>,
) -> anyhow::Result<()> {
    match commands {
        toml_edit::Item::ArrayOfTables(tables) => {
            for (index, command) in tables.iter_mut().enumerate() {
                rename_table_fields(
                    command,
                    &format!("{scope}[{index}]"),
                    &SNAKE_CASE_FIELDS[6..],
                    migrated,
                )?;
            }
        }
        toml_edit::Item::Value(toml_edit::Value::Array(commands)) => {
            for (index, command) in commands.iter_mut().enumerate() {
                if let toml_edit::Value::InlineTable(command) = command {
                    rename_table_fields(
                        command,
                        &format!("{scope}[{index}]"),
                        &SNAKE_CASE_FIELDS[6..],
                        migrated,
                    )?;
                }
            }
        }
        toml_edit::Item::Table(command) => {
            rename_table_fields(command, scope, &SNAKE_CASE_FIELDS[6..], migrated)?;
        }
        _ => {}
    }
    Ok(())
}

fn rename_table_fields(
    table: &mut dyn toml_edit::TableLike,
    scope: &str,
    fields: &[(&str, &str)],
    migrated: &mut BTreeSet<String>,
) -> anyhow::Result<()> {
    let renames = fields
        .iter()
        .filter(|(legacy, _)| table.contains_key(legacy))
        .copied()
        .collect::<Vec<_>>();

    for (legacy, current) in &renames {
        if table.contains_key(current) {
            anyhow::bail!(t!(
                "cli.config.snake_case_conflict",
                scope = scope,
                legacy = legacy,
                current = current
            ));
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

fn plan_channel_update(
    content: &str,
    channel: Option<&str>,
    bump: Option<ChannelBumpArg>,
    requested: &[String],
    all: bool,
) -> anyhow::Result<MigrationPlan> {
    let mut document = content.parse::<toml_edit::DocumentMut>()?;
    let packages = document
        .get_mut("packages")
        .and_then(toml_edit::Item::as_table_mut)
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
        let table = packages
            .get_mut(&name)
            .with_context(|| t!("cli.config.package_not_configured", package = name))?
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
                match bump {
                    Some(bump) => {
                        table.insert("channel-bump", toml_edit::value(bump.as_str()));
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

    use camino::Utf8PathBuf;
    use clap::Parser as _;
    use semifold_engine::Project;
    use semifold_resolver::{config, resolver::ResolverType};

    use super::{
        ChannelBumpArg, ChannelTarget, Config as ConfigCommand, ConfigPathError, Migrate, Sync,
        migrate, plan_channel_update, plan_migration, sync, toml_config_path, update_channel,
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

    fn project_from_config_path(config_path: PathBuf) -> Project {
        let config = config::load_config(&config_path).unwrap();
        project_with_config_path(config_path, config)
    }

    fn project_with_config_path(config_path: PathBuf, config: config::Config) -> Project {
        let changeset_root = config_path
            .parent()
            .expect("test config has a parent directory")
            .to_path_buf();
        let root = changeset_root
            .parent()
            .unwrap_or(&changeset_root)
            .to_path_buf();
        Project {
            root: Utf8PathBuf::from_path_buf(root).unwrap(),
            changeset_dir: Utf8PathBuf::from_path_buf(changeset_root).unwrap(),
            config_path: Utf8PathBuf::from_path_buf(config_path.clone()).unwrap(),
            config,
        }
    }

    fn sync_project(root: &std::path::Path) -> Project {
        let changeset_root = root.join(".changes");
        let config_path = changeset_root.join("config.toml");
        project_from_config_path(config_path)
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
    fn migrates_known_snake_case_fields_recursively_and_preserves_comments() {
        let plan = plan_migration(
            r#"
[packages.app]
# retained package comment
path = "."
resolver = "rust"
channel_bump = "preserve"
depends_on = ["core"]

[tags]
dry_run = "User-defined tag"

[resolver.rust]
post_version = [
  { command = "cargo", args = ["generate-lockfile"], dry_run = true },
]

[resolver.rust.pre_check]
url = "https://example.test/{{ package.name }}"
extra_headers = { User_Agent = "semifold", dry_run = "header-value" }

[[resolver.rust.publish]]
command = "cargo"
extra_env = { TOKEN = "secret", dry_run = "environment-value" }
dry_run = true
"#,
        )
        .unwrap();

        assert!(plan.content.contains("# retained package comment"));
        assert!(plan.content.contains("channel-bump = \"preserve\""));
        assert!(plan.content.contains("depends-on = [\"core\"]"));
        assert!(plan.content.contains("[resolver.rust.pre-check]"));
        assert!(
            plan.content.contains(
                "extra-headers = { User_Agent = \"semifold\", dry_run = \"header-value\" }"
            )
        );
        assert!(
            plan.content
                .contains("extra-env = { TOKEN = \"secret\", dry_run = \"environment-value\" }")
        );
        assert!(plan.content.contains("dry_run = \"User-defined tag\""));
        assert!(plan.content.contains("dry-run = true"));
        assert!(plan.content.contains("post-version = ["));
        for legacy in [
            "channel_bump",
            "depends_on",
            "pre_check",
            "extra_headers",
            "extra_env",
            "post_version",
        ] {
            assert!(
                !plan.content.contains(legacy),
                "legacy field remained: {legacy}"
            );
        }
        assert!(!plan.packages.is_empty());
        assert!(plan_migration(&plan.content).unwrap().packages.is_empty());
    }

    #[test]
    fn rejects_snake_and_kebab_case_conflicts() {
        let error = plan_migration(
            r#"
[packages.app]
path = "."
resolver = "rust"
channel_bump = "preserve"
channel-bump = "patch"
"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("channel_bump and channel-bump"));
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
        let project = project_from_config_path(path.clone());

        let error = migrate(&Migrate { check: true }, &project, false).unwrap_err();

        assert!(
            error.to_string().contains("migration is required"),
            "unexpected error: {error:#}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn migrate_accepts_snake_case_before_validating_the_result() {
        let path = temporary_config_path("snake-case");
        let original = r#"
[branches]
base = "main"
release = "release"

[packages.app]
path = "."
resolver = "rust"

[tags]

[resolver.rust.pre_check]
url = ""
"#;
        fs::write(&path, original).unwrap();
        let project = project_from_config_path(path.clone());

        migrate(&Migrate { check: false }, &project, false).unwrap();

        let migrated = fs::read_to_string(&path).unwrap();
        assert!(migrated.contains("[resolver.rust.pre-check]"));
        assert!(!migrated.contains("pre_check"));
        config::load_config(&path).unwrap();
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
        let set = plan_channel_update(content, Some("alpha"), None, &requested, false).unwrap();

        assert_eq!(set.packages, ["app"]);
        assert!(set.content.contains("# retained"));
        assert!(set.content.contains("assets = [\"dist/*\"]"));
        assert!(set.content.contains("channel = \"alpha\""));
        assert!(set.content.contains("[packages.library]"));
        assert!(set.content.contains("channel = \"beta\""));
        assert!(
            plan_channel_update(&set.content, Some("alpha"), None, &requested, false)
                .unwrap()
                .packages
                .is_empty()
        );

        let clear = plan_channel_update(&set.content, None, None, &requested, false).unwrap();
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
            None,
            &[],
            true,
        )
        .unwrap();

        assert_eq!(plan.packages, ["app", "library"]);
        assert_eq!(plan.content.matches("channel = \"alpha\"").count(), 2);
    }

    #[test]
    fn channel_bump_is_written_only_when_the_channel_changes() {
        let content = r#"
[packages.app]
path = "."
resolver = "rust"

[packages.already]
path = "already"
resolver = "rust"
channel = "alpha"
"#;
        let all = plan_channel_update(
            content,
            Some("alpha"),
            Some(ChannelBumpArg::Preserve),
            &[],
            true,
        )
        .unwrap();

        assert_eq!(all.packages, ["app"]);
        assert_eq!(
            all.content.matches("channel-bump = \"preserve\"").count(),
            1
        );
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
        let project = project_from_config_path(path.clone());
        let target = ChannelTarget {
            packages: vec!["app".to_string()],
            all: false,
            check: true,
        };

        let error = update_channel(Some("alpha"), None, &target, &project, false).unwrap_err();

        assert!(error.to_string().contains("do not match"));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn sync_adds_discovered_packages_and_is_idempotent() {
        let root = temporary_sync_root("write");
        let context = sync_project(&root);
        let config_path = root.join(".changes/config.toml");

        sync(
            &Sync {
                check: false,
                prune: false,
                resolvers: vec![],
            },
            &context,
            false,
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
            &sync_project(&root),
            false,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), first);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_dry_run_does_not_write_the_configuration() {
        let root = temporary_sync_root("dry-run");
        let context = sync_project(&root);
        let config_path = root.join(".changes/config.toml");
        let original = fs::read_to_string(&config_path).unwrap();

        sync(
            &Sync {
                check: false,
                prune: true,
                resolvers: vec![],
            },
            &context,
            true,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_check_reports_drift_without_writing_the_configuration() {
        let root = temporary_sync_root("check");
        let context = sync_project(&root);
        let config_path = root.join(".changes/config.toml");
        let original = fs::read_to_string(&config_path).unwrap();

        let error = sync(
            &Sync {
                check: true,
                prune: false,
                resolvers: vec![],
            },
            &context,
            false,
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
            &sync_project(&root),
            false,
        )
        .unwrap();

        sync(
            &Sync {
                check: true,
                prune: false,
                resolvers: vec![],
            },
            &sync_project(&root),
            false,
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
            &sync_project(&root),
            false,
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

        sync(&rust_only, &sync_project(&root), false).unwrap();
        let synced = fs::read_to_string(&config_path).unwrap();
        assert!(synced.contains("[packages.app]"));
        assert!(synced.contains("[packages.node-only]"));

        sync(
            &Sync {
                check: true,
                prune: false,
                resolvers: vec![ResolverType::Rust],
            },
            &sync_project(&root),
            false,
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
            &sync_project(&root),
            false,
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
            &sync_project(&root),
            false,
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

    #[test]
    fn sync_rejects_json_configuration_with_a_typed_format_error() {
        let path = temporary_config_path("json").with_extension("json");
        fs::write(&path, "{}").unwrap();
        let project = project_with_config_path(
            path.clone(),
            config::Config {
                branches: config::BranchesConfig {
                    base: "main".to_string(),
                    release: "release".to_string(),
                },
                tags: Default::default(),
                packages: Default::default(),
                resolver: Default::default(),
            },
        );

        assert_eq!(
            toml_config_path(&project),
            Err(ConfigPathError::UnsupportedConfigFormat)
        );
        let error = sync(
            &Sync {
                check: false,
                prune: false,
                resolvers: vec![],
            },
            &project,
            false,
        )
        .unwrap_err();
        assert!(error.to_string().contains("supports only TOML"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
