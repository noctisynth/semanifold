use crate::PackageId;

/// An internal package dependency discovered by an ecosystem adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dependency {
    pub package: PackageId,
    pub kind: DependencyKind,
    pub requirement: Option<String>,
}

/// The semantic role of a package dependency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyKind {
    Runtime,
    Development,
    Build,
    Optional,
    Peer,
}
