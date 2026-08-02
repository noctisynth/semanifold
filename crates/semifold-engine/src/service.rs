use std::{fs::OpenOptions, io::Write};

use camino::{Utf8Path, Utf8PathBuf};
use semifold_core::{ConfigSyncPlan, ReleasePlan};
use semifold_resolver::{error::ResolveError, resolver};
use thiserror::Error;

use crate::{
    config_editor::{ConfigEditError, TomlConfigEditor},
    config_sync::{ConfigSyncPlanningError, config_sync_scope, plan_config_sync},
    project::Project,
    publish_plan::{PublishOptions, PublishPlan},
    publisher::{CommandRunner, SystemCommandRunner},
    publisher::{
        ForgeExecution, GithubForgeClient, HttpRegistryClient, PublishExecutionError,
        PublishReport, SystemAssetResolver, SystemFileSystem, execute_publish_plan,
    },
    release,
    release_apply::{
        ApplyReport, ExecutionMode, ReleaseApplyError, ReleaseApplyPlan, ReleaseExecutionOptions,
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

pub trait EngineDependencies {
    fn write_atomic(&self, path: &Utf8Path, content: &str) -> Result<(), std::io::Error>;
    fn remove_file(&self, path: &Utf8Path) -> Result<(), std::io::Error>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemDependencies;

impl EngineDependencies for SystemDependencies {
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

    pub fn plan_release(&self, project: &Project) -> Result<ReleasePlan, AppError> {
        let changesets =
            resolver::get_changesets(project.changeset_dir.as_std_path(), &project.config)?;
        release::plan_release(project.root.as_std_path(), &project.config, &changesets)
            .map_err(AppError::ReleasePlan)
    }
}

impl<D: EngineDependencies> SemifoldService<D> {
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
        crate::release_apply::apply_release(&self.deps, plan, mode).map_err(AppError::ReleaseApply)
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
                    .map_err(|error| AppError::PublishSetup(anyhow::Error::from(error)))?
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
            &HttpRegistryClient::default(),
            forge,
            mode == ExecutionMode::DryRun,
        )
        .await
        .map_err(AppError::PublishExecution)
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("failed to create changeset: {0}")]
    ChangesetCreate(#[source] crate::changeset_service::ChangesetCreateError),
    #[error("failed to load changesets")]
    Changesets(#[from] ResolveError),
    #[error("failed to plan configuration synchronization")]
    ConfigSyncPlanning(#[from] ConfigSyncPlanningError),
    #[error("failed to edit configuration")]
    ConfigEdit(#[from] ConfigEditError),
    #[error("failed to write configuration {path}")]
    ConfigWrite {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to plan release: {0}")]
    ReleasePlan(#[source] anyhow::Error),
    #[error("failed to prepare release: {0}")]
    ReleasePrepare(#[source] anyhow::Error),
    #[error("failed to apply release: {0}")]
    ReleaseApply(#[source] ReleaseApplyError),
    #[error("failed to plan publish: {0}")]
    PublishPlan(#[source] anyhow::Error),
    #[error("failed to initialize publish dependencies: {0}")]
    PublishSetup(#[source] anyhow::Error),
    #[error("failed to execute publish plan: {0}")]
    PublishExecution(#[source] PublishExecutionError),
}
