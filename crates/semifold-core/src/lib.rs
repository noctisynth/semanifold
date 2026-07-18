//! Cross-ecosystem release domain types.

pub mod dependency;
pub mod package;
pub mod workspace;

pub use dependency::{Dependency, DependencyKind};
pub use package::{Ecosystem, PackageId, PackageSnapshot};
pub use workspace::{WorkspaceGraph, WorkspaceGraphError};
