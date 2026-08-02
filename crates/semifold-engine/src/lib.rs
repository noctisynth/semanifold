#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod config_editor;
pub mod config_sync;
pub mod discovery;
pub mod file_edit_executor;
pub mod package_path;
pub mod project;
pub mod publish_plan;
pub mod publisher;
pub mod release;
pub mod release_apply;
pub mod service;
pub mod workspace;

pub use project::{Project, ProjectLoadError, ProjectLocation};
pub use release_apply::{
    ApplyReport, ExecutionMode, PostVersionCommand, PostVersionFailure, ReleaseApplyError,
    ReleaseApplyPlan, ReleaseExecutionOptions,
};
pub use service::{
    AppError, ConfigSyncOptions, ConfigSyncReport, SemifoldService, SystemDependencies,
};
