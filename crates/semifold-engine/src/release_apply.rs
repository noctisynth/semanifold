use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};
use semifold_changelog::{ChangelogSource, generate_changelog, utils::render_changelog};
use semifold_core::{
    ChangesetId, DependencyUpdateContext, EditSource, FileEdit, FileEditExpectation, FileHash,
    PackageId, ReleaseContext, ReleasePackageContext, ReleasePlan, ReleaseReason,
    RepositoryContext,
};
use semifold_resolver::{
    changeset::Changeset,
    config::{PackageConfig, StdioType},
};
use thiserror::Error;

use crate::{
    file_edit_executor::{
        FileEditApplyError, FileEditApplyReport, FileEditExecutor, validate_file_edits,
    },
    project::Project,
    publish_plan::{CommandPhase, CommandSpec, StdioPolicy},
    publisher::{CommandError, CommandRunner},
    workspace::load_workspace_graph,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReleaseExecutionOptions {
    pub collect_remote_metadata: bool,
    pub repository: Option<RepositoryContext>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionMode {
    Apply,
    DryRun,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostVersionCommand {
    pub package: PackageId,
    pub command: CommandSpec,
}

#[derive(Debug)]
pub struct ReleaseApplyPlan {
    pub release: ReleasePlan,
    pub project_root: Utf8PathBuf,
    pub config_path: Utf8PathBuf,
    pub changelogs: BTreeMap<PackageId, String>,
    pub changesets_to_remove: Vec<Utf8PathBuf>,
    pub channel_bumps_to_consume: Vec<PackageId>,
    pub post_version_commands: Vec<PostVersionCommand>,
    pub remote_metadata_failures: Vec<PackageId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ApplyReport {
    pub changelogs: BTreeMap<PackageId, String>,
    pub file_edits: Option<FileEditApplyReport>,
    pub unconsumed_changesets: Vec<ChangesetId>,
}

#[derive(Debug)]
pub struct PostVersionFailure {
    pub package: PackageId,
    pub command: CommandSpec,
    pub source: CommandError,
}

#[derive(Debug, Error)]
pub enum ReleaseApplyError {
    #[error("release file validation failed")]
    FileValidation(#[source] FileEditApplyError),
    #[error("release file application failed")]
    FileApply(#[source] FileEditApplyError),
    #[error("post-version command failed for {failure_package}", failure_package = .failure.package)]
    PostVersion {
        report: ApplyReport,
        failure: PostVersionFailure,
    },
    #[error("failed to update one-shot channel bumps")]
    ChannelBump(#[source] anyhow::Error),
    #[error("failed to remove consumed changeset {path}")]
    RemoveChangeset {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub async fn prepare_release(
    project: &Project,
    changesets: &[Changeset],
    release: ReleasePlan,
    options: &ReleaseExecutionOptions,
) -> anyhow::Result<ReleaseApplyPlan> {
    let repository = git2::Repository::open(&project.root)?;
    let workspace = load_workspace_graph(project.root.as_std_path(), &project.config)?;
    let release_context = ReleaseContext::from_plan(&release);
    let mut file_edits = release.file_edits().to_vec();
    let mut changelogs = BTreeMap::new();
    let mut remote_metadata_failures = Vec::new();

    for package_id in release.order() {
        let package_name = package_id.as_str();
        let package_config =
            project.config.packages.get(package_name).ok_or_else(|| {
                anyhow::anyhow!("release package {package_name} is not configured")
            })?;
        let package_release = release
            .package(package_id)
            .ok_or_else(|| anyhow::anyhow!("release package {package_name} is not planned"))?;
        let dependency_updates = package_release
            .reasons
            .iter()
            .filter_map(|reason| match reason {
                ReleaseReason::DependencyPropagation { dependency, .. } => release
                    .versions()
                    .get(dependency)
                    .map(|version| DependencyUpdateContext {
                        package: dependency.clone(),
                        next_version: version.clone(),
                    }),
                ReleaseReason::Changeset { .. }
                | ReleaseReason::SharedVersionPropagation { .. } => None,
            })
            .collect::<Vec<_>>();
        let snapshot = workspace.package(package_id).ok_or_else(|| {
            anyhow::anyhow!("release package {package_name} is missing from workspace")
        })?;
        let package_context = ReleasePackageContext::from_snapshot(&release_context, snapshot)?;
        let changelog = generate_changelog(
            &ChangelogSource {
                repo_root: project.root.as_std_path(),
                tags: &project.config.tags,
                repository: options.repository.as_ref(),
            },
            &repository,
            changesets,
            package_context,
            dependency_updates,
            options.collect_remote_metadata,
        )
        .await?;
        if changelog.remote_metadata_failed {
            remote_metadata_failures.push(package_id.clone());
        }
        file_edits.push(plan_changelog_edit(
            &project.root,
            package_config,
            package_id,
            &changelog.content,
        )?);
        changelogs.insert(package_id.clone(), changelog.content);
    }

    let release = release.with_file_edits(file_edits)?;
    validate_file_edits(&project.root, release.file_edits())?;
    let changesets_to_remove = consumed_changeset_paths(changesets, release.consumed_changesets())?;
    let channel_bumps_to_consume = release
        .packages()
        .iter()
        .filter(|package| {
            package.current_version.pre.is_empty()
                && project
                    .config
                    .packages
                    .get(package.id.as_str())
                    .is_some_and(|config| {
                        config.channel_bump.is_some() && !config.channel.is_stable()
                    })
        })
        .map(|package| package.id.clone())
        .collect();

    Ok(ReleaseApplyPlan {
        release,
        project_root: project.root.clone(),
        config_path: project.config_path.clone(),
        changelogs,
        changesets_to_remove,
        channel_bumps_to_consume,
        post_version_commands: plan_post_version_commands(project)?,
        remote_metadata_failures,
    })
}

pub fn apply_release<D>(
    deps: &D,
    plan: ReleaseApplyPlan,
    mode: ExecutionMode,
) -> Result<ApplyReport, ReleaseApplyError>
where
    D: crate::service::EngineDependencies + CommandRunner,
{
    validate_file_edits(&plan.project_root, plan.release.file_edits())
        .map_err(ReleaseApplyError::FileValidation)?;
    let changeset_ids = plan.release.consumed_changesets().to_vec();

    if mode == ExecutionMode::DryRun {
        run_post_version_commands(deps, &plan.post_version_commands, mode, None, &plan)?;
        return Ok(ApplyReport {
            changelogs: plan.changelogs,
            file_edits: None,
            unconsumed_changesets: changeset_ids,
        });
    }

    let file_edits = FileEditExecutor::new(&plan.project_root)
        .apply(plan.release.file_edits())
        .map_err(ReleaseApplyError::FileApply)?;
    run_post_version_commands(
        deps,
        &plan.post_version_commands,
        mode,
        Some(file_edits.clone()),
        &plan,
    )?;
    consume_channel_bumps(deps, &plan.config_path, &plan.channel_bumps_to_consume)
        .map_err(ReleaseApplyError::ChannelBump)?;
    for path in &plan.changesets_to_remove {
        deps.remove_file(path)
            .map_err(|source| ReleaseApplyError::RemoveChangeset {
                path: path.clone(),
                source,
            })?;
    }

    Ok(ApplyReport {
        changelogs: plan.changelogs,
        file_edits: Some(file_edits),
        unconsumed_changesets: Vec::new(),
    })
}

fn run_post_version_commands<D>(
    deps: &D,
    commands: &[PostVersionCommand],
    mode: ExecutionMode,
    file_edits: Option<FileEditApplyReport>,
    plan: &ReleaseApplyPlan,
) -> Result<(), ReleaseApplyError>
where
    D: CommandRunner,
{
    for planned in commands {
        if mode == ExecutionMode::DryRun && !planned.command.run_in_dry_run {
            continue;
        }
        if let Err(source) = deps.run(&planned.command) {
            return Err(ReleaseApplyError::PostVersion {
                report: ApplyReport {
                    changelogs: plan.changelogs.clone(),
                    file_edits,
                    unconsumed_changesets: plan.release.consumed_changesets().to_vec(),
                },
                failure: PostVersionFailure {
                    package: planned.package.clone(),
                    command: planned.command.clone(),
                    source,
                },
            });
        }
    }
    Ok(())
}

fn consumed_changeset_paths(
    changesets: &[Changeset],
    consumed: &[ChangesetId],
) -> anyhow::Result<Vec<Utf8PathBuf>> {
    consumed
        .iter()
        .map(|id| {
            let changeset = changesets
                .iter()
                .find(|changeset| changeset.name == id.as_str())
                .ok_or_else(|| anyhow::anyhow!("planned changeset {} is missing", id.as_str()))?;
            let path = changeset.path.as_ref().ok_or_else(|| {
                anyhow::anyhow!("planned changeset {} has no source path", id.as_str())
            })?;
            Utf8PathBuf::from_path_buf(path.clone())
                .map_err(|path| anyhow::anyhow!("changeset path is not valid UTF-8: {path:?}"))
        })
        .collect()
}

fn plan_post_version_commands(project: &Project) -> anyhow::Result<Vec<PostVersionCommand>> {
    let mut commands = Vec::new();
    for (package_name, package_config) in &project.config.packages {
        let Some(resolver_config) = project.config.resolver.get(&package_config.resolver) else {
            continue;
        };
        let working_directory = project.root.join(
            Utf8Path::from_path(&package_config.path)
                .ok_or_else(|| anyhow::anyhow!("package path is not valid UTF-8"))?,
        );
        for command in &resolver_config.post_version {
            commands.push(PostVersionCommand {
                package: PackageId::new(package_name),
                command: CommandSpec {
                    executable: command.command.clone(),
                    args: command.args.clone().unwrap_or_default(),
                    environment: command.extra_env.clone(),
                    working_directory: working_directory.clone(),
                    phase: CommandPhase::PostVersion,
                    stdout: stdio_policy(command.stdout),
                    stderr: stdio_policy(command.stderr),
                    run_in_dry_run: command.dry_run.unwrap_or(false),
                },
            });
        }
    }
    Ok(commands)
}

fn plan_changelog_edit(
    root: &Utf8Path,
    package_config: &PackageConfig,
    package: &PackageId,
    entry: &str,
) -> anyhow::Result<FileEdit> {
    let relative_path = Utf8PathBuf::from_path_buf(package_config.path.join("CHANGELOG.md"))
        .map_err(|path| anyhow::anyhow!("changelog path is not valid UTF-8: {path:?}"))?;
    let absolute_path = root.join(&relative_path);
    let content = match std::fs::read_to_string(&absolute_path) {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let expected = content
        .as_ref()
        .map_or(FileEditExpectation::Missing, |content| {
            FileEditExpectation::Existing {
                hash: FileHash::from_bytes(content.as_bytes()),
            }
        });
    let new_content = render_changelog(&absolute_path, content.as_deref(), entry)?;

    Ok(FileEdit {
        path: relative_path,
        expected,
        new_content,
        source: EditSource::Changelog {
            package: package.clone(),
        },
    })
}

fn consume_channel_bumps<D>(deps: &D, path: &Utf8Path, packages: &[PackageId]) -> anyhow::Result<()>
where
    D: crate::service::EngineDependencies,
{
    if packages.is_empty() {
        return Ok(());
    }
    let original = std::fs::read_to_string(path)?;
    let mut document = original.parse::<toml_edit::DocumentMut>()?;
    let configured = document["packages"]
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("configuration is missing the packages table"))?;
    for package in packages {
        let table = configured
            .get_mut(package.as_str())
            .and_then(toml_edit::Item::as_table_like_mut)
            .ok_or_else(|| anyhow::anyhow!("configured package {package} is not a table"))?;
        table.remove("channel-bump");
    }
    let content = document.to_string();
    if content != original {
        semifold_resolver::config::load_config_from_str(path.as_std_path(), &content)?;
        deps.write_atomic(path, &content)?;
    }
    Ok(())
}

const fn stdio_policy(stdio: StdioType) -> StdioPolicy {
    match stdio {
        StdioType::Inherit => StdioPolicy::Inherit,
        StdioType::Pipe => StdioPolicy::Pipe,
        StdioType::Null => StdioPolicy::Null,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use semifold_core::{FileEditExpectation, PackageId};
    use semifold_resolver::{
        config::{PackageConfig, ReleaseChannel},
        resolver::ResolverType,
    };

    use super::plan_changelog_edit;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn temporary_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock in tests must be after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "semifold-release-apply-{}-{nonce}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("release apply fixture directory must be created");
        root
    }

    fn package_config() -> PackageConfig {
        PackageConfig {
            path: "package".into(),
            resolver: ResolverType::Nodejs,
            channel: ReleaseChannel::Stable,
            channel_bump: None,
            assets: Vec::new(),
            depends_on: Vec::new(),
        }
    }

    #[test]
    fn plans_new_and_existing_changelog_edits() {
        let root = temporary_root();
        fs::create_dir_all(root.join("package"))
            .expect("package fixture directory must be created");
        let root = camino::Utf8Path::from_path(&root)
            .expect("temporary test directory must be valid UTF-8");
        let package = package_config();

        let new_edit = plan_changelog_edit(root, &package, &PackageId::new("app"), "## v1.0.0")
            .expect("new changelog edit must be planned");
        assert_eq!(new_edit.path.as_str(), "package/CHANGELOG.md");
        assert_eq!(new_edit.expected, FileEditExpectation::Missing);
        assert_eq!(new_edit.new_content, "# Changelog\n\n## v1.0.0\n");

        fs::write(root.join("package/CHANGELOG.md"), "# Changelog\n")
            .expect("existing changelog fixture must be written");
        let existing_edit =
            plan_changelog_edit(root, &package, &PackageId::new("app"), "## v1.0.0")
                .expect("existing changelog edit must be planned");
        assert!(matches!(
            existing_edit.expected,
            FileEditExpectation::Existing { .. }
        ));
        assert_eq!(existing_edit.new_content, "# Changelog\n\n## v1.0.0\n");
        fs::remove_dir_all(root).expect("release apply fixture must be removed");
    }
}
