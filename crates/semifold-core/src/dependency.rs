use crate::PackageId;

/// An internal package dependency discovered by an ecosystem adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dependency {
    pub package: PackageId,
    pub kind: DependencyKind,
    pub requirement: Option<String>,
    pub source: DependencySource,
}

/// The semantic role of a package dependency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyKind {
    /// A configured graph edge without an ecosystem-specific dependency role.
    Unspecified,
    Runtime,
    Development,
    Build,
    Optional,
    Peer,
}

/// Where an internal dependency edge was declared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencySource {
    Manifest,
    Config,
}
