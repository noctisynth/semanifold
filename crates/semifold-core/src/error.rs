use crate::{
    ReleasePackageContextError, ReleasePlanError, ReleasePlannerError, VersioningError,
    WorkspaceGraphError,
};

/// Aggregate error boundary for callers that compose multiple core domain operations.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DomainError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceGraphError),
    #[error(transparent)]
    ReleasePlanning(#[from] ReleasePlannerError),
    #[error(transparent)]
    ReleasePlan(#[from] ReleasePlanError),
    #[error(transparent)]
    Versioning(#[from] VersioningError),
    #[error(transparent)]
    ReleaseContext(#[from] ReleasePackageContextError),
}
