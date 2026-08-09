use camino::{Utf8Path, Utf8PathBuf};
use semifold_core::{
    DependencyKind, EcosystemId, FileEdit, PackageId, PackageSnapshot, VersionMap, VersionSource,
};
use semver::Version;
use std::path::PathBuf;

use crate::error::ResolveError;

/// A configured package location that an ecosystem adapter can inspect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageLocation {
    pub id: PackageId,
    pub project_root: Utf8PathBuf,
    pub path: Utf8PathBuf,
}

/// A dependency declaration whose manifest name has not yet been bound to a stable package id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestDependency {
    pub manifest_name: String,
    pub kind: DependencyKind,
    pub requirement: Option<String>,
}

/// Manifest data used internally while an adapter discovers or inspects a package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedPackage {
    pub name: String,
    pub version: Version,
    pub version_source: VersionSource,
    pub path: PathBuf,
    pub private: bool,
}

/// Immutable package data parsed by an adapter before workspace dependency binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInspection {
    pub id: PackageId,
    pub manifest_name: String,
    pub version: Version,
    pub version_source: VersionSource,
    pub ecosystem: EcosystemId,
    pub path: Utf8PathBuf,
    pub publishable: bool,
    pub dependencies: Vec<ManifestDependency>,
}

/// Immutable, complete input for one ecosystem's edit planning pass.
#[derive(Clone, Copy, Debug)]
pub struct EcosystemPlanInput<'input> {
    pub project_root: &'input Utf8Path,
    pub workspace_packages: &'input [PackageSnapshot],
    pub released_packages: &'input [PackageId],
    pub versions: &'input VersionMap,
}

/// Side-effect-free package discovery, inspection, and file-edit planning.
pub trait EcosystemAdapter: Send + Sync {
    fn ecosystem(&self) -> EcosystemId;

    /// Validates a planned domain version and encodes it for this ecosystem's manifests.
    fn encode_version(&self, version: &Version) -> Result<String, AdapterError>;

    fn discover(&self, root: &Utf8Path) -> Result<Vec<PackageInspection>, AdapterError>;

    fn inspect(&self, package: &PackageLocation) -> Result<PackageInspection, AdapterError>;

    fn plan_edits(&self, input: EcosystemPlanInput<'_>) -> Result<Vec<FileEdit>, AdapterError>;
}

/// Failures produced at an ecosystem adapter boundary.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error(transparent)]
    Manifest(#[from] ResolveError),
    #[error("invalid adapter input: {reason}")]
    InvalidInput { reason: String },
    #[error(
        "{ecosystem_name} cannot encode version {version}: {reason}",
        ecosystem_name = .ecosystem.display_name()
    )]
    InvalidVersion {
        ecosystem: EcosystemId,
        version: Version,
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use semver::Version;

    use super::*;

    struct ContractAdapter;

    impl EcosystemAdapter for ContractAdapter {
        fn ecosystem(&self) -> EcosystemId {
            EcosystemId::NODE
        }

        fn encode_version(&self, version: &Version) -> Result<String, AdapterError> {
            Ok(version.to_string())
        }

        fn discover(&self, _root: &Utf8Path) -> Result<Vec<PackageInspection>, AdapterError> {
            Ok(Vec::new())
        }

        fn inspect(&self, package: &PackageLocation) -> Result<PackageInspection, AdapterError> {
            Ok(PackageInspection {
                id: package.id.clone(),
                manifest_name: "example".to_string(),
                version: Version::new(1, 0, 0),
                version_source: VersionSource::PackageManifest,
                ecosystem: self.ecosystem(),
                path: package.path.clone(),
                publishable: true,
                dependencies: Vec::new(),
            })
        }

        fn plan_edits(&self, input: EcosystemPlanInput<'_>) -> Result<Vec<FileEdit>, AdapterError> {
            if input
                .released_packages
                .iter()
                .all(|package| input.versions.contains_key(package))
            {
                Ok(Vec::new())
            } else {
                Err(AdapterError::InvalidInput {
                    reason: "released package is missing from VersionMap".to_string(),
                })
            }
        }
    }

    #[test]
    fn adapter_contract_is_object_safe_and_receives_complete_plan_input() {
        let adapter: Box<dyn EcosystemAdapter> = Box::new(ContractAdapter);
        let location = PackageLocation {
            id: PackageId::new("configured-id"),
            project_root: "/project".into(),
            path: "packages/example".into(),
        };
        let inspection = adapter.inspect(&location).unwrap();
        let snapshot = PackageSnapshot {
            id: inspection.id.clone(),
            manifest_name: inspection.manifest_name.clone(),
            version: inspection.version.clone(),
            version_source: inspection.version_source.clone(),
            ecosystem: inspection.ecosystem,
            path: inspection.path.clone(),
            publishable: inspection.publishable,
            dependencies: Vec::new(),
        };
        let versions = BTreeMap::from([(PackageId::new("configured-id"), Version::new(1, 0, 1))]);

        assert_eq!(inspection.id, PackageId::new("configured-id"));
        assert_eq!(inspection.path, "packages/example");
        assert!(
            adapter
                .plan_edits(EcosystemPlanInput {
                    project_root: &location.project_root,
                    workspace_packages: std::slice::from_ref(&snapshot),
                    released_packages: std::slice::from_ref(&snapshot.id),
                    versions: &versions,
                })
                .unwrap()
                .is_empty()
        );
    }
}
