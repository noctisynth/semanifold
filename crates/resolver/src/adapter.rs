use camino::{Utf8Path, Utf8PathBuf};
use semifold_core::{Ecosystem, FileEdit, PackageId, PackageSnapshot, VersionMap};

use crate::error::ResolveError;

/// A configured package location that an ecosystem adapter can inspect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageLocation {
    pub id: PackageId,
    pub project_root: Utf8PathBuf,
    pub path: Utf8PathBuf,
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
    fn ecosystem(&self) -> Ecosystem;

    fn discover(&self, root: &Utf8Path) -> Result<Vec<PackageSnapshot>, AdapterError>;

    fn inspect(&self, package: &PackageLocation) -> Result<PackageSnapshot, AdapterError>;

    fn plan_edits(&self, input: EcosystemPlanInput<'_>) -> Result<Vec<FileEdit>, AdapterError>;
}

/// Failures produced at an ecosystem adapter boundary.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error(transparent)]
    LegacyResolver(#[from] ResolveError),
    #[error("invalid adapter input: {reason}")]
    InvalidInput { reason: String },
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use semifold_core::Dependency;
    use semver::Version;

    use super::*;

    struct ContractAdapter;

    impl EcosystemAdapter for ContractAdapter {
        fn ecosystem(&self) -> Ecosystem {
            Ecosystem::Node
        }

        fn discover(&self, _root: &Utf8Path) -> Result<Vec<PackageSnapshot>, AdapterError> {
            Ok(Vec::new())
        }

        fn inspect(&self, package: &PackageLocation) -> Result<PackageSnapshot, AdapterError> {
            Ok(PackageSnapshot {
                id: package.id.clone(),
                manifest_name: "example".to_string(),
                version: Version::new(1, 0, 0),
                ecosystem: self.ecosystem(),
                path: package.path.clone(),
                publishable: true,
                dependencies: Vec::<Dependency>::new(),
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
        let snapshot = adapter.inspect(&location).unwrap();
        let versions = BTreeMap::from([(PackageId::new("configured-id"), Version::new(1, 0, 1))]);

        assert_eq!(snapshot.id, PackageId::new("configured-id"));
        assert_eq!(snapshot.path, "packages/example");
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
