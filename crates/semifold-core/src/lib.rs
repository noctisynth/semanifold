//! Cross-ecosystem release domain types.
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod changeset;
pub mod config_sync;
pub mod dependency;
pub mod file_edit;
pub mod package;
pub mod planner;
pub mod release_context;
pub mod release_plan;
pub mod versioning;
pub mod workspace;

pub use changeset::{BumpLevel, ChangesetId, PlanWarning, ReleaseReason};
pub use config_sync::{
    ChangesetReference, ConfigConflict, ConfigSyncPlan, ConfigSyncPlanner, ConfigSyncWarning,
    ConfiguredPackage, DiscoveredPackage, PackageMove, PackageRename,
};
pub use dependency::{Dependency, DependencyKind, DependencySource};
pub use file_edit::{EditSource, FileEdit, FileEditExpectation, FileHash, SharedVersionEdit};
pub use package::{Ecosystem, PackageId, PackageSnapshot, VersionSource, VersionSourceId};
pub use planner::{
    ChangesetInput, PackageReleasePolicy, ReleasePlanner, ReleasePlannerError, ReleasePolicies,
};
pub use release_context::{
    ChangelogContext, ChangesetContext, CiContext, CiProvider, CommitContext,
    DependencyUpdateContext, PackageChangesetContext, PackageReleaseContext, PullRequestContext,
    ReleaseContext, ReleasePackageContext, ReleasePackageContextError,
    ReleasePackageTemplateContext, ReleasePlanContext, ReleaseReasonContext, RepositoryContext,
};
pub use release_plan::{PackageRelease, ReleasePlan, ReleasePlanError, VersionMap};
pub use versioning::{ReleaseChannel, VersioningError, bump_version};
pub use workspace::{WorkspaceGraph, WorkspaceGraphError};
