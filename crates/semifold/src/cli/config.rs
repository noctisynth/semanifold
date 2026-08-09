use anyhow::anyhow;
use camino::Utf8Path;
use clap::{Parser, Subcommand, ValueEnum};
use rust_i18n::t;
use semifold_core::{ConfigSyncWarning, EcosystemId};
use semifold_engine::{
    AppError, ChannelUpdate, ConfigMutationError, ConfigSyncOptions, Project, ProjectLocation,
    SemifoldService, SystemDependencies, config_sync::ConfigSyncPlanningError,
};
use semifold_resolver::config::ChannelBump;

use crate::cli::terminal::{StepOutcome, Terminal};

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
        help = t!("cli.config.flags.resolver_sync")
    )]
    resolvers: Vec<EcosystemId>,
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
    const fn as_channel_bump(self) -> ChannelBump {
        match self {
            Self::Preserve => ChannelBump::Preserve,
            Self::Patch => ChannelBump::Patch,
            Self::Minor => ChannelBump::Minor,
            Self::Major => ChannelBump::Major,
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

pub(crate) fn run(command: &Config, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    render_header(dry_run);
    match &command.command {
        Commands::Sync(options) => sync(options, project, dry_run),
        Commands::Migrate(options) => migrate(options, &project.config_path, dry_run),
        Commands::Channel(channel) => manage_channel(channel, project, dry_run),
    }
}

pub(crate) fn run_before_project_load(
    command: &Config,
    location: &ProjectLocation,
    dry_run: bool,
) -> Option<anyhow::Result<()>> {
    let Commands::Migrate(options) = &command.command else {
        return None;
    };
    Some((|| {
        render_header(dry_run);
        let config_path = location
            .config_path()
            .map_err(|error| anyhow!(t!("cli.project_load_failed", error = error)))?;
        migrate(options, config_path, dry_run)
    })())
}

fn render_header(dry_run: bool) {
    let terminal = Terminal::detect();
    terminal.heading(&t!("cli.config.heading"));
    if dry_run {
        terminal.dry_run(&t!("cli.common.dry_run_banner"));
    }
}

fn sync(options: &Sync, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    let terminal = Terminal::detect();
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

    report_sync_warnings(&plan.warnings, &terminal);
    if !plan.missing.is_empty() {
        terminal.warning(&t!(
            "cli.config.sync_missing",
            packages = plan
                .missing
                .iter()
                .map(|package| package.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if options.check {
        if plan.has_drift() {
            anyhow::bail!(t!("cli.config.sync_check_failed"));
        }
        terminal.summary(StepOutcome::Success, &t!("cli.config.sync_check_passed"));
        return Ok(());
    }
    if dry_run {
        terminal.section(&t!("cli.config.sync_dry_run"));
        terminal.line(serde_json::to_string_pretty(&plan)?);
        terminal.summary(StepOutcome::Success, &t!("cli.config.dry_run_complete"));
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
        terminal.summary(StepOutcome::Skipped, &t!("cli.config.sync_not_required"));
        return Ok(());
    }

    let report = service
        .apply_config_sync(&plan)
        .map_err(|error| anyhow!(t!("cli.config.sync_edit_failed", error = error)))?;
    if !report.changed {
        terminal.summary(StepOutcome::Skipped, &t!("cli.config.sync_no_safe_changes"));
        return Ok(());
    }
    terminal.summary(
        StepOutcome::Success,
        &t!("cli.config.synced", path = report.path),
    );
    Ok(())
}

fn render_config_sync_error(error: AppError) -> anyhow::Error {
    match error {
        AppError::UnsupportedConfigFormat => anyhow!(t!(
            "cli.config.unsupported_format",
            command = t!("cli.config.command_sync")
        )),
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

fn report_sync_warnings(warnings: &[ConfigSyncWarning], terminal: &Terminal) {
    for warning in warnings {
        match warning {
            ConfigSyncWarning::ChangesetReferencesRenamedPackage {
                changeset,
                from,
                to,
            } => terminal.warning(&t!(
                "cli.config.sync_renamed_changeset_warning",
                changeset = changeset.as_str(),
                from = from.as_str(),
                to = to.as_str()
            )),
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
    let terminal = Terminal::detect();
    let service = SemifoldService::new(SystemDependencies);
    let plan = service
        .plan_channel_update(
            project,
            &ChannelUpdate {
                channel: channel.map(ToOwned::to_owned),
                bump: bump.map(ChannelBumpArg::as_channel_bump),
                packages: target.packages.clone(),
                all: target.all,
            },
        )
        .map_err(|error| {
            render_config_mutation_error(error, t!("cli.config.command_channel").as_ref())
        })?;
    if plan.packages.is_empty() {
        terminal.summary(
            StepOutcome::Skipped,
            &t!("cli.config.channels_already_match"),
        );
        return Ok(());
    }

    if let Some(channel) = channel {
        let packages = node_packages_missing_npm_dist_tag(project, &plan.packages);
        if !packages.is_empty() {
            terminal.warning(&t!(
                "cli.config.node_npm_channel_tag_missing",
                packages = packages.join(", "),
                channel = channel
            ));
        }
    }

    let requested = channel.unwrap_or("stable");
    terminal.line(t!(
        "cli.config.updating_channel",
        channel = requested,
        packages = plan.packages.join(", ")
    ));
    if target.check {
        anyhow::bail!(t!("cli.config.channels_mismatch"));
    }
    if dry_run {
        terminal.summary(StepOutcome::Success, &t!("cli.config.dry_run_complete"));
        return Ok(());
    }

    let report = service.apply_config_mutation(&plan)?;
    terminal.summary(
        StepOutcome::Success,
        &t!("cli.config.updated", path = report.path),
    );
    Ok(())
}

fn node_packages_missing_npm_dist_tag(
    project: &Project,
    target_packages: &[String],
) -> Vec<String> {
    let Some(resolver) = project.config.resolver.get(&EcosystemId::NODE) else {
        return Vec::new();
    };
    if !resolver.publish.iter().any(npm_publish_missing_dist_tag) {
        return Vec::new();
    }
    target_packages
        .iter()
        .filter(|package| {
            project
                .config
                .packages
                .get(package.as_str())
                .is_some_and(|config| config.resolver == EcosystemId::NODE)
        })
        .cloned()
        .collect()
}

fn npm_publish_missing_dist_tag(command: &semifold_resolver::config::CommandConfig) -> bool {
    if command.command != "npm" {
        return false;
    }
    let Some(arguments) = command.args.as_deref() else {
        return false;
    };
    let publishes = arguments.iter().any(|argument| argument == "publish");
    let has_dist_tag = has_npm_dist_tag(arguments);
    publishes && !has_dist_tag
}

fn has_npm_dist_tag(arguments: &[String]) -> bool {
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if argument
            .strip_prefix("--tag=")
            .is_some_and(|tag| !tag.is_empty())
        {
            return true;
        }
        if argument == "--tag"
            && arguments
                .next()
                .is_some_and(|tag| !tag.is_empty() && !tag.starts_with('-'))
        {
            return true;
        }
    }
    false
}

fn migrate(options: &Migrate, config_path: &Utf8Path, dry_run: bool) -> anyhow::Result<()> {
    let terminal = Terminal::detect();
    let service = SemifoldService::new(SystemDependencies);
    let plan = service
        .plan_config_migration_at(config_path)
        .map_err(|error| {
            render_config_mutation_error(error, t!("cli.config.command_migrate").as_ref())
        })?;
    if plan.packages.is_empty() {
        terminal.summary(
            StepOutcome::Skipped,
            &t!("cli.config.migration_not_required"),
        );
        return Ok(());
    }

    terminal.line(t!(
        "cli.config.migration_required",
        packages = plan.packages.join(", ")
    ));
    if options.check {
        anyhow::bail!(t!("cli.config.migration_required_error"));
    }
    if dry_run {
        terminal.summary(StepOutcome::Success, &t!("cli.config.dry_run_complete"));
        return Ok(());
    }

    let report = service.apply_config_mutation(&plan)?;
    terminal.summary(
        StepOutcome::Success,
        &t!("cli.config.migrated", path = report.path),
    );
    Ok(())
}

fn render_config_mutation_error(error: AppError, command: &str) -> anyhow::Error {
    match error {
        AppError::UnsupportedConfigFormat => {
            anyhow!(t!("cli.config.unsupported_format", command = command))
        }
        AppError::ConfigMutation(ConfigMutationError::MissingPackagesTable) => {
            anyhow!(t!("cli.config.missing_packages_table"))
        }
        AppError::ConfigMutation(ConfigMutationError::PackageNotTable { package }) => {
            anyhow!(t!("cli.config.package_must_be_table", package = package))
        }
        AppError::ConfigMutation(ConfigMutationError::ChannelLegacyConflict { package }) => {
            anyhow!(t!("cli.config.channel_legacy_conflict", package = package))
        }
        AppError::ConfigMutation(ConfigMutationError::InvalidLegacyVersionMode {
            package, ..
        }) => anyhow!(t!(
            "cli.config.invalid_legacy_version_mode",
            package = package
        )),
        AppError::ConfigMutation(ConfigMutationError::SnakeCaseConflict {
            scope,
            legacy,
            current,
        }) => anyhow!(t!(
            "cli.config.snake_case_conflict",
            scope = scope,
            legacy = legacy,
            current = current
        )),
        AppError::ConfigMutation(ConfigMutationError::PackageNotConfigured { package }) => {
            anyhow!(t!("cli.config.package_not_configured", package = package))
        }
        error => anyhow!(error),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use camino::Utf8PathBuf;
    use clap::Parser as _;
    use semifold_core::EcosystemId;
    use semifold_engine::{
        ChannelUpdate, ConfigMutationError, ConfigMutationPlan, Project,
        config_management::{
            plan_channel_update as engine_plan_channel_update,
            plan_config_migration as engine_plan_config_migration,
        },
    };
    use semifold_resolver::config;

    use super::{
        ChannelBumpArg, ChannelTarget, Config as ConfigCommand, Migrate, Sync, migrate,
        node_packages_missing_npm_dist_tag, sync, update_channel,
    };

    fn plan_migration(content: &str) -> Result<ConfigMutationPlan, ConfigMutationError> {
        engine_plan_config_migration("config.toml".into(), content)
    }

    fn plan_channel_update(
        content: &str,
        channel: Option<&str>,
        bump: Option<ChannelBumpArg>,
        packages: &[String],
        all: bool,
    ) -> Result<ConfigMutationPlan, ConfigMutationError> {
        engine_plan_channel_update(
            "config.toml".into(),
            content,
            &ChannelUpdate {
                channel: channel.map(ToOwned::to_owned),
                bump: bump.map(ChannelBumpArg::as_channel_bump),
                packages: packages.to_vec(),
                all,
            },
        )
    }

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
            "[branches]\nbase = \"main\"\nrelease = \"release\"\n\n[tags]\n\n[packages]\n\n[resolver.rust.pre-check]\ntype = \"http\"\nurl = \"\"\n",
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
                "{}\n[resolver.nodejs.pre-check]\ntype = \"http\"\nurl = \"\"\n",
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

        let error = migrate(&Migrate { check: true }, &project.config_path, false).unwrap_err();

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

        migrate(&Migrate { check: false }, &project.config_path, false).unwrap();

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
    fn identifies_only_targeted_node_packages_when_npm_publish_lacks_a_dist_tag() {
        let config_path = PathBuf::from("config.toml");
        let config = config::load_config_from_str(
            &config_path,
            r#"
[branches]
base = "main"
release = "release"

[tags]

[packages.node-app]
path = "node-app"
resolver = "nodejs"

[packages.rust-app]
path = "rust-app"
resolver = "rust"

[[resolver.nodejs.publish]]
command = "npm"
args = ["publish"]
"#,
        )
        .unwrap();
        let project = project_with_config_path(config_path, config);

        assert_eq!(
            node_packages_missing_npm_dist_tag(
                &project,
                &["node-app".to_string(), "rust-app".to_string()]
            ),
            ["node-app"]
        );
    }

    #[test]
    fn accepts_both_npm_dist_tag_argument_forms() {
        for arguments in [
            "[\"publish\", \"--tag\", \"alpha\"]",
            "[\"publish\", \"--tag=alpha\"]",
        ] {
            let config_path = PathBuf::from("config.toml");
            let config = config::load_config_from_str(
                &config_path,
                &format!(
                    r#"
[branches]
base = "main"
release = "release"

[tags]

[packages.app]
path = "."
resolver = "nodejs"

[[resolver.nodejs.publish]]
command = "npm"
args = {arguments}
"#
                ),
            )
            .unwrap();
            let project = project_with_config_path(config_path, config);

            assert!(node_packages_missing_npm_dist_tag(&project, &["app".to_string()]).is_empty());
        }
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
        assert!(
            ConfigCommand::try_parse_from(["config", "sync", "--resolver", "com.example.game",])
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
            resolvers: vec![EcosystemId::RUST],
        };

        sync(&rust_only, &sync_project(&root), false).unwrap();
        let synced = fs::read_to_string(&config_path).unwrap();
        assert!(synced.contains("[packages.app]"));
        assert!(synced.contains("[packages.node-only]"));

        sync(
            &Sync {
                check: true,
                prune: false,
                resolvers: vec![EcosystemId::RUST],
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
                resolvers: vec![EcosystemId::NODE],
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
                resolvers: vec![EcosystemId::RUST],
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
                changelog: Default::default(),
                packages: Default::default(),
                plugins: Default::default(),
                resolver: Default::default(),
            },
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
