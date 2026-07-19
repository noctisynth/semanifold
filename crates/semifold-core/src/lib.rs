//! Cross-ecosystem release domain types.

pub mod changeset;
pub mod dependency;
pub mod file_edit;
pub mod package;
pub mod release_plan;
pub mod workspace;

pub use changeset::{BumpLevel, ChangesetId, PlanWarning, ReleaseReason};
pub use dependency::{Dependency, DependencyKind};
pub use file_edit::{EditSource, FileEdit, FileHash};
pub use package::{Ecosystem, PackageId, PackageSnapshot};
pub use release_plan::{PackageRelease, ReleasePlan, ReleasePlanError, VersionMap};
pub use workspace::{WorkspaceGraph, WorkspaceGraphError};
