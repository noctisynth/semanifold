//! Cross-ecosystem release domain types.
#![cfg_attr(not(test), deny(clippy::expect_used, clippy::unwrap_used))]

pub mod changeset;
pub mod config_sync;
pub mod dependency;
pub mod file_edit;
pub mod package;
pub mod planner;
pub mod release_plan;
pub mod versioning;
pub mod workspace;

pub use changeset::{BumpLevel, ChangesetId, PlanWarning, ReleaseReason};
pub use config_sync::{
    ChangesetReference, ConfigConflict, ConfigSyncPlan, ConfigSyncPlanner, ConfigSyncWarning,
    ConfiguredPackage, DiscoveredPackage, PackageMove, PackageRename,
};
pub use dependency::{Dependency, DependencyKind};
pub use file_edit::{EditSource, FileEdit, FileEditExpectation, FileHash};
pub use package::{Ecosystem, PackageId, PackageSnapshot};
pub use planner::{
    ChangesetInput, PackageReleasePolicy, ReleasePlanner, ReleasePlannerError, ReleasePolicies,
};
pub use release_plan::{PackageRelease, ReleasePlan, ReleasePlanError, VersionMap};
pub use versioning::{ReleaseChannel, VersioningError, bump_version};
pub use workspace::{WorkspaceGraph, WorkspaceGraphError};
