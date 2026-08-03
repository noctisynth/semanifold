use std::{fs::OpenOptions, io::Write};

use camino::{Utf8Path, Utf8PathBuf};
use semifold_core::{ConfigSyncPlan, ReleasePlan};
use semifold_resolver::{error::ResolveError, resolver};
use thiserror::Error;

use crate::{
    config_editor::{ConfigEditError, TomlConfigEditor},
    config_management::{
        ChannelUpdate, ConfigMutationError, ConfigMutationPlan, plan_channel_update,
        plan_config_migration,
    },
    config_sync::{ConfigSyncPlanningError, config_sync_scope, plan_config_sync},
    init::{InitOptions, InitPlan, InitPlanningError, InitReport},
    project::Project,
    publish_plan::{PublishOptions, PublishPlan, PublishPlanError},
    publisher::{CommandRunner, SystemCommandRunner},
    publisher::{
        ForgeExecution, GithubForgeClient, PublishExecutionError, PublishReport,
        SystemAssetResolver, SystemFileSystem, SystemPreCheckRunner, execute_publish_plan,
    },
    release::{self, ReleasePlanningError},
    release_apply::{
        ApplyReport, ExecutionMode, ReleaseApplyError, ReleaseApplyPlan, ReleaseExecutionOptions,
        ReleasePrepareError,
    },
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConfigSyncOptions {
    pub resolvers: Vec<semifold_resolver::resolver::ResolverType>,
    pub prune_missing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigSyncReport {
    pub path: Utf8PathBuf,
    pub changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigMutationReport {
    pub path: Utf8PathBuf,
    pub changed: bool,
}

pub trait EngineDependencies {
    fn create_dir_all(&self, path: &Utf8Path) -> Result<(), std::io::Error>;
    fn write_atomic(&self, path: &Utf8Path, content: &str) -> Result<(), std::io::Error>;
    fn remove_file(&self, path: &Utf8Path) -> Result<(), std::io::Error>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemDependencies;

impl EngineDependencies for SystemDependencies {
    fn create_dir_all(&self, path: &Utf8Path) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(path)
    }

    fn write_atomic(&self, path: &Utf8Path, content: &str) -> Result<(), std::io::Error> {
        let extension = path.extension().unwrap_or("tmp");
        let temporary = path.with_extension(format!("{extension}.{}.tmp", std::process::id()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }

    fn remove_file(&self, path: &Utf8Path) -> Result<(), std::io::Error> {
        std::fs::remove_file(path)
    }
}

impl CommandRunner for SystemDependencies {
    fn run(
        &self,
        command: &crate::publish_plan::CommandSpec,
    ) -> Result<crate::publisher::CommandOutput, crate::publisher::CommandError> {
        SystemCommandRunner.run(command)
    }
}

pub struct SemifoldService<D> {
    deps: D,
}

impl<D> SemifoldService<D> {
    pub const fn new(deps: D) -> Self {
        Self { deps }
    }

    pub fn plan_init(
        &self,
        location: &crate::project::ProjectLocation,
        options: InitOptions,
    ) -> Result<InitPlan, AppError> {
        crate::init::plan_init(location, options).map_err(AppError::InitPlanning)
    }

    pub fn ensure_clean_worktree(
        &self,
        project: &Project,
        allow_dirty: bool,
    ) -> Result<(), AppError> {
        if allow_dirty {
            return Ok(());
        }
        let repository =
            git2::Repository::open(project.root.as_std_path()).map_err(AppError::GitOpen)?;
        let statuses = repository.statuses(None).map_err(AppError::GitStatus)?;
        if statuses.iter().all(|entry| {
            matches!(
                entry.status(),
                git2::Status::CURRENT | git2::Status::IGNORED
            )
        }) {
            Ok(())
        } else {
            Err(AppError::DirtyWorktree)
        }
    }

    pub fn create_changeset(
        &self,
        project: &Project,
        draft: crate::changeset_service::ChangesetDraft,
    ) -> Result<semifold_core::ChangesetId, AppError> {
        crate::changeset_service::create_changeset(project, draft)
            .map_err(AppError::ChangesetCreate)
    }

    pub fn plan_config_sync(
        &self,
        project: &Project,
        options: &ConfigSyncOptions,
    ) -> Result<ConfigSyncPlan, AppError> {
        ensure_toml_config(project)?;
        let scope = config_sync_scope(&project.config, &options.resolvers)?;
        let changesets =
            resolver::get_changesets(project.changeset_dir.as_std_path(), &project.config)?;
        Ok(plan_config_sync(
            project.root.as_std_path(),
            project.config_path.as_std_path(),
            &project.config,
            &changesets,
            &scope,
            options.prune_missing,
        )?)
    }

    pub fn plan_config_migration(&self, project: &Project) -> Result<ConfigMutationPlan, AppError> {
        let content = read_editable_config(project)?;
        let plan = plan_config_migration(project.config_path.clone(), &content)
            .map_err(AppError::ConfigMutation)?;
        validate_config_mutation(&plan)?;
        Ok(plan)
    }

    pub fn plan_channel_update(
        &self,
        project: &Project,
        update: &ChannelUpdate,
    ) -> Result<ConfigMutationPlan, AppError> {
        let content = read_editable_config(project)?;
        let plan = plan_channel_update(project.config_path.clone(), &content, update)
            .map_err(AppError::ConfigMutation)?;
        validate_config_mutation(&plan)?;
        Ok(plan)
    }

    pub fn plan_release(&self, project: &Project) -> Result<ReleasePlan, AppError> {
        let changesets =
            resolver::get_changesets(project.changeset_dir.as_std_path(), &project.config)?;
        release::plan_release(project.root.as_std_path(), &project.config, &changesets)
            .map_err(AppError::ReleasePlan)
    }
}

impl<D: EngineDependencies> SemifoldService<D> {
    pub fn apply_init(&self, plan: &InitPlan) -> Result<InitReport, AppError> {
        for directory in &plan.directories {
            self.deps
                .create_dir_all(directory)
                .map_err(|source| AppError::InitDirectory {
                    path: directory.clone(),
                    source,
                })?;
        }
        for file in &plan.files {
            self.deps
                .write_atomic(&file.path, &file.content)
                .map_err(|source| AppError::InitWrite {
                    path: file.path.clone(),
                    source,
                })?;
        }
        Ok(InitReport {
            files: plan.files.iter().map(|file| file.path.clone()).collect(),
        })
    }

    pub fn apply_config_sync(&self, plan: &ConfigSyncPlan) -> Result<ConfigSyncReport, AppError> {
        let mut editor = TomlConfigEditor::load(&plan.config_path)?;
        let original = editor.render();
        editor.apply(plan)?;
        let content = editor.render();
        let changed = content != original;
        if changed {
            self.deps
                .write_atomic(&plan.config_path, &content)
                .map_err(|source| AppError::ConfigWrite {
                    path: plan.config_path.clone(),
                    source,
                })?;
        }
        Ok(ConfigSyncReport {
            path: plan.config_path.clone(),
            changed,
        })
    }

    pub fn apply_config_mutation(
        &self,
        plan: &ConfigMutationPlan,
    ) -> Result<ConfigMutationReport, AppError> {
        self.deps
            .write_atomic(&plan.path, &plan.content)
            .map_err(|source| AppError::ConfigWrite {
                path: plan.path.clone(),
                source,
            })?;
        Ok(ConfigMutationReport {
            path: plan.path.clone(),
            changed: true,
        })
    }
}

fn read_editable_config(project: &Project) -> Result<String, AppError> {
    ensure_toml_config(project)?;
    std::fs::read_to_string(&project.config_path).map_err(|source| {
        AppError::ConfigMutation(ConfigMutationError::Read {
            path: project.config_path.clone(),
            source,
        })
    })
}

fn ensure_toml_config(project: &Project) -> Result<(), AppError> {
    if project.config_path.extension() == Some("toml") {
        Ok(())
    } else {
        Err(AppError::UnsupportedConfigFormat)
    }
}

fn validate_config_mutation(plan: &ConfigMutationPlan) -> Result<(), AppError> {
    semifold_resolver::config::load_config_from_str(plan.path.as_std_path(), &plan.content)
        .map(|_| ())
        .map_err(|source| AppError::ConfigMutation(ConfigMutationError::InvalidResult(source)))
}

impl<D> SemifoldService<D>
where
    D: EngineDependencies + CommandRunner,
{
    pub async fn prepare_release(
        &self,
        project: &Project,
        release: ReleasePlan,
        options: &ReleaseExecutionOptions,
    ) -> Result<ReleaseApplyPlan, AppError> {
        let changesets =
            resolver::get_changesets(project.changeset_dir.as_std_path(), &project.config)?;
        crate::release_apply::prepare_release(project, &changesets, release, options)
            .await
            .map_err(AppError::ReleasePrepare)
    }

    pub fn apply_release(
        &self,
        plan: ReleaseApplyPlan,
        mode: ExecutionMode,
    ) -> Result<ApplyReport, AppError> {
        crate::release_apply::apply_release(&self.deps, plan, mode)
            .map_err(|error| AppError::ReleaseApply(Box::new(error)))
    }
}

impl SemifoldService<SystemDependencies> {
    pub async fn plan_publish(
        &self,
        project: &Project,
        options: &PublishOptions,
    ) -> Result<PublishPlan, AppError> {
        crate::publish_plan::plan_publish(project.root.as_std_path(), &project.config, options)
            .await
            .map_err(AppError::PublishPlan)
    }

    pub async fn publish(
        &self,
        mut plan: PublishPlan,
        mode: ExecutionMode,
    ) -> Result<PublishReport, AppError> {
        let forge_client = if plan.packages.iter().any(|package| package.forge.is_some()) {
            let client = if let Ok(token) = std::env::var("GITHUB_TOKEN") {
                octocrab::Octocrab::builder()
                    .personal_token(token)
                    .build()
                    .map_err(AppError::PublishSetup)?
            } else {
                octocrab::Octocrab::default()
            };
            Some(GithubForgeClient::new(client))
        } else {
            None
        };
        let file_system = SystemFileSystem;
        let asset_resolver = SystemAssetResolver;
        let project_root = plan.project_root.clone();
        let forge = forge_client.as_ref().map(|client| ForgeExecution {
            client,
            file_system: &file_system,
            asset_resolver: &asset_resolver,
            root: project_root.as_std_path(),
        });
        execute_publish_plan(
            &mut plan,
            &self.deps,
            &SystemPreCheckRunner::default(),
            forge,
            mode == ExecutionMode::DryRun,
        )
        .await
        .map_err(|error| AppError::PublishExecution(Box::new(error)))
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration format is not supported for editing")]
    UnsupportedConfigFormat,
    #[error("failed to create changeset: {0}")]
    ChangesetCreate(#[source] crate::changeset_service::ChangesetCreateError),
    #[error("failed to load changesets")]
    Changesets(#[from] ResolveError),
    #[error("failed to plan initialization: {0}")]
    InitPlanning(#[source] InitPlanningError),
    #[error("failed to create initialization directory {path}")]
    InitDirectory {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write initialization file {path}")]
    InitWrite {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to open Git repository")]
    GitOpen(#[source] git2::Error),
    #[error("failed to inspect Git worktree status")]
    GitStatus(#[source] git2::Error),
    #[error("Git worktree contains uncommitted changes")]
    DirtyWorktree,
    #[error("failed to plan configuration synchronization")]
    ConfigSyncPlanning(#[from] ConfigSyncPlanningError),
    #[error("failed to edit configuration")]
    ConfigEdit(#[from] ConfigEditError),
    #[error("failed to mutate configuration: {0}")]
    ConfigMutation(#[source] ConfigMutationError),
    #[error("failed to write configuration {path}")]
    ConfigWrite {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to plan release: {0}")]
    ReleasePlan(#[source] ReleasePlanningError),
    #[error("failed to prepare release: {0}")]
    ReleasePrepare(#[source] ReleasePrepareError),
    #[error("failed to apply release: {0}")]
    ReleaseApply(#[source] Box<ReleaseApplyError>),
    #[error("failed to plan publish: {0}")]
    PublishPlan(#[source] PublishPlanError),
    #[error("failed to initialize publish dependencies: {0}")]
    PublishSetup(#[source] octocrab::Error),
    #[error("failed to execute publish plan: {0}")]
    PublishExecution(#[source] Box<PublishExecutionError>),
}
