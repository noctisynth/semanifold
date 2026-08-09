use std::collections::{BTreeMap, BTreeSet};

use semifold_core::{EcosystemId, PackageId};
use semver::Version;
use serde::{Deserialize, Serialize};

pub const PLUGIN_PROTOCOL_SCHEMA_VERSION: u32 = 1;

const REQUIRED_OPERATIONS: [PluginOperation; 3] = [
    PluginOperation::Discover,
    PluginOperation::Inspect,
    PluginOperation::PlanEdits,
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginOperation {
    Discover,
    Inspect,
    PlanEdits,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginMetadataV1 {
    pub schema_version: u32,
    pub ecosystem: EcosystemId,
    pub plugin_version: Version,
    pub operations: BTreeSet<PluginOperation>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub read_patterns: BTreeSet<String>,
}

impl PluginMetadataV1 {
    #[must_use]
    pub fn new(
        ecosystem: EcosystemId,
        plugin_version: Version,
        read_patterns: BTreeSet<String>,
    ) -> Self {
        Self {
            schema_version: PLUGIN_PROTOCOL_SCHEMA_VERSION,
            ecosystem,
            plugin_version,
            operations: REQUIRED_OPERATIONS.into_iter().collect(),
            read_patterns,
        }
    }

    pub fn validate(&self) -> Result<(), PluginProtocolError> {
        validate_schema_version(self.schema_version)?;
        if self.ecosystem.is_builtin() {
            return Err(PluginProtocolError::BuiltInEcosystemReserved {
                ecosystem: self.ecosystem.clone(),
            });
        }
        for operation in REQUIRED_OPERATIONS {
            if !self.operations.contains(&operation) {
                return Err(PluginProtocolError::MissingOperation { operation });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginRequestV1 {
    pub schema_version: u32,
    #[serde(flatten)]
    pub call: PluginCallV1,
}

impl PluginRequestV1 {
    #[must_use]
    pub const fn new(call: PluginCallV1) -> Self {
        Self {
            schema_version: PLUGIN_PROTOCOL_SCHEMA_VERSION,
            call,
        }
    }

    #[must_use]
    pub const fn operation(&self) -> PluginOperation {
        self.call.operation()
    }

    pub fn validate(&self) -> Result<(), PluginProtocolError> {
        validate_schema_version(self.schema_version)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", content = "input", rename_all = "kebab-case")]
pub enum PluginCallV1 {
    Discover(PluginDiscoverInputV1),
    Inspect(PluginInspectInputV1),
    PlanEdits(PluginPlanEditsInputV1),
}

impl PluginCallV1 {
    #[must_use]
    pub const fn operation(&self) -> PluginOperation {
        match self {
            Self::Discover(_) => PluginOperation::Discover,
            Self::Inspect(_) => PluginOperation::Inspect,
            Self::PlanEdits(_) => PluginOperation::PlanEdits,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginDiscoverInputV1 {
    pub project_root: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginInspectInputV1 {
    pub project_root: String,
    pub package: PluginPackageLocationV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginPlanEditsInputV1 {
    pub project_root: String,
    pub workspace_packages: Vec<PluginPackageSnapshotV1>,
    pub released_packages: Vec<PackageId>,
    pub versions: BTreeMap<PackageId, Version>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginPackageLocationV1 {
    pub id: PackageId,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginPackageInspectionV1 {
    pub id: PackageId,
    pub manifest_name: String,
    pub version: Version,
    pub version_source: PluginVersionSourceV1,
    pub ecosystem: EcosystemId,
    pub path: String,
    pub publishable: bool,
    pub dependencies: Vec<PluginManifestDependencyV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginPackageSnapshotV1 {
    pub id: PackageId,
    pub manifest_name: String,
    pub version: Version,
    pub version_source: PluginVersionSourceV1,
    pub ecosystem: EcosystemId,
    pub path: String,
    pub publishable: bool,
    pub dependencies: Vec<PluginDependencyV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PluginVersionSourceV1 {
    PackageManifest,
    Shared { manifest: String, field: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginManifestDependencyV1 {
    pub manifest_name: String,
    pub kind: PluginDependencyKindV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginDependencyV1 {
    pub package: PackageId,
    pub kind: PluginDependencyKindV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
    pub source: PluginDependencySourceV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginDependencyKindV1 {
    Unspecified,
    Runtime,
    Development,
    Build,
    Optional,
    Peer,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginDependencySourceV1 {
    Manifest,
    Config,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginResponseV1 {
    pub schema_version: u32,
    pub diagnostics: Vec<PluginDiagnosticV1>,
    #[serde(flatten)]
    pub outcome: PluginOutcomeV1,
}

impl PluginResponseV1 {
    pub fn validate_for(
        &self,
        request: &PluginRequestV1,
        plugin: &EcosystemId,
    ) -> Result<(), PluginProtocolError> {
        validate_schema_version(self.schema_version)?;
        request.validate()?;
        for diagnostic in &self.diagnostics {
            if diagnostic.plugin != *plugin {
                return Err(PluginProtocolError::DiagnosticPluginMismatch {
                    expected: plugin.clone(),
                    actual: diagnostic.plugin.clone(),
                });
            }
            if diagnostic.operation != request.operation() {
                return Err(PluginProtocolError::DiagnosticOperationMismatch {
                    expected: request.operation(),
                    actual: diagnostic.operation,
                });
            }
        }

        match &self.outcome {
            PluginOutcomeV1::Success { output } => {
                let actual = output.operation();
                let expected = request.operation();
                if actual != expected {
                    return Err(PluginProtocolError::ResponseOperationMismatch {
                        expected,
                        actual,
                    });
                }
            }
            PluginOutcomeV1::Failure => {
                if !self
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == PluginDiagnosticSeverityV1::Error)
                {
                    return Err(PluginProtocolError::FailureWithoutErrorDiagnostic);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum PluginOutcomeV1 {
    Success { output: Box<PluginOutputV1> },
    Failure,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", content = "output", rename_all = "kebab-case")]
pub enum PluginOutputV1 {
    Discover {
        packages: Vec<PluginPackageInspectionV1>,
    },
    Inspect {
        package: PluginPackageInspectionV1,
    },
    PlanEdits {
        edits: Vec<PluginFileEditV1>,
    },
}

impl PluginOutputV1 {
    #[must_use]
    pub const fn operation(&self) -> PluginOperation {
        match self {
            Self::Discover { .. } => PluginOperation::Discover,
            Self::Inspect { .. } => PluginOperation::Inspect,
            Self::PlanEdits { .. } => PluginOperation::PlanEdits,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginFileEditV1 {
    pub path: String,
    pub expected: PluginFileEditExpectationV1,
    pub new_content: String,
    pub source: PluginEditSourceV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PluginFileEditExpectationV1 {
    Existing { sha256: String },
    Missing,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PluginEditSourceV1 {
    PackageVersion {
        package: PackageId,
    },
    DependencyVersion {
        package: PackageId,
        dependency: PackageId,
    },
    WorkspaceDependencies {
        dependencies: Vec<PackageId>,
    },
    WorkspaceManifest {
        shared_versions: Vec<PluginSharedVersionEditV1>,
        dependencies: Vec<PackageId>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginSharedVersionEditV1 {
    pub manifest: String,
    pub field: String,
    pub packages: Vec<PackageId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginDiagnosticV1 {
    pub plugin: EcosystemId,
    pub operation: PluginOperation,
    pub severity: PluginDiagnosticSeverityV1,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginDiagnosticSeverityV1 {
    Info,
    Warning,
    Error,
}

fn validate_schema_version(actual: u32) -> Result<(), PluginProtocolError> {
    if actual == PLUGIN_PROTOCOL_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(PluginProtocolError::UnsupportedSchemaVersion {
            expected: PLUGIN_PROTOCOL_SCHEMA_VERSION,
            actual,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PluginProtocolError {
    #[error("unsupported plugin protocol schema version {actual}; expected {expected}")]
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
    #[error("plugin ecosystem id {ecosystem} is reserved for a built-in ecosystem")]
    BuiltInEcosystemReserved { ecosystem: EcosystemId },
    #[error("plugin metadata does not declare required operation {operation:?}")]
    MissingOperation { operation: PluginOperation },
    #[error("plugin response operation {actual:?} does not match request {expected:?}")]
    ResponseOperationMismatch {
        expected: PluginOperation,
        actual: PluginOperation,
    },
    #[error("plugin diagnostic identifies {actual}, but the registered plugin is {expected}")]
    DiagnosticPluginMismatch {
        expected: EcosystemId,
        actual: EcosystemId,
    },
    #[error("plugin diagnostic operation {actual:?} does not match request {expected:?}")]
    DiagnosticOperationMismatch {
        expected: PluginOperation,
        actual: PluginOperation,
    },
    #[error("a failed plugin response must include at least one error diagnostic")]
    FailureWithoutErrorDiagnostic,
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use semifold_core::{EcosystemId, PackageId};
    use semver::Version;
    use serde_json::json;

    use super::*;

    fn plugin_id() -> EcosystemId {
        EcosystemId::new("com.example.engine").unwrap()
    }

    #[test]
    fn metadata_requires_current_schema_custom_identity_and_complete_operations() {
        let metadata = PluginMetadataV1::new(
            plugin_id(),
            Version::new(1, 2, 3),
            BTreeSet::from(["manifests/**/*.json".to_string()]),
        );
        assert_eq!(metadata.validate(), Ok(()));
        assert_eq!(
            serde_json::to_value(&metadata).unwrap(),
            json!({
                "schema-version": 1,
                "ecosystem": "com.example.engine",
                "plugin-version": "1.2.3",
                "operations": ["discover", "inspect", "plan-edits"],
                "read-patterns": ["manifests/**/*.json"]
            })
        );

        let mut missing = metadata.clone();
        missing.operations.remove(&PluginOperation::Inspect);
        assert_eq!(
            missing.validate(),
            Err(PluginProtocolError::MissingOperation {
                operation: PluginOperation::Inspect
            })
        );

        let built_in = PluginMetadataV1::new(
            EcosystemId::new("rust").unwrap(),
            Version::new(1, 0, 0),
            BTreeSet::new(),
        );
        assert!(matches!(
            built_in.validate(),
            Err(PluginProtocolError::BuiltInEcosystemReserved { .. })
        ));
    }

    #[test]
    fn discover_request_has_a_stable_versioned_json_shape() {
        let request = PluginRequestV1::new(PluginCallV1::Discover(PluginDiscoverInputV1 {
            project_root: ".".to_string(),
        }));
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(
            value,
            json!({
                "schema-version": 1,
                "operation": "discover",
                "input": { "project-root": "." }
            })
        );
        assert_eq!(
            serde_json::from_value::<PluginRequestV1>(value).unwrap(),
            request
        );
    }

    #[test]
    fn plan_edits_request_preserves_sorted_version_facts() {
        let request = PluginRequestV1::new(PluginCallV1::PlanEdits(PluginPlanEditsInputV1 {
            project_root: ".".to_string(),
            workspace_packages: Vec::new(),
            released_packages: vec![PackageId::new("app")],
            versions: BTreeMap::from([
                (PackageId::new("zeta"), Version::new(2, 0, 0)),
                (PackageId::new("app"), Version::new(1, 1, 0)),
            ]),
        }));
        let serialized = serde_json::to_string(&request).unwrap();

        assert!(serialized.contains(r#""versions":{"app":"1.1.0","zeta":"2.0.0"}"#));
        assert_eq!(
            serde_json::from_str::<PluginRequestV1>(&serialized).unwrap(),
            request
        );
    }

    #[test]
    fn response_validation_binds_diagnostics_and_output_to_the_request() {
        let plugin = plugin_id();
        let request = PluginRequestV1::new(PluginCallV1::Inspect(PluginInspectInputV1 {
            project_root: ".".to_string(),
            package: PluginPackageLocationV1 {
                id: PackageId::new("game"),
                path: "game".to_string(),
            },
        }));
        let package = PluginPackageInspectionV1 {
            id: PackageId::new("game"),
            manifest_name: "game".to_string(),
            version: Version::new(1, 0, 0),
            version_source: PluginVersionSourceV1::PackageManifest,
            ecosystem: plugin.clone(),
            path: "game".to_string(),
            publishable: true,
            dependencies: Vec::new(),
        };
        let response = PluginResponseV1 {
            schema_version: PLUGIN_PROTOCOL_SCHEMA_VERSION,
            diagnostics: vec![PluginDiagnosticV1 {
                plugin: plugin.clone(),
                operation: PluginOperation::Inspect,
                severity: PluginDiagnosticSeverityV1::Warning,
                code: "manifest-field-deprecated".to_string(),
                message: "The legacy field remains readable.".to_string(),
                package: Some(PackageId::new("game")),
                path: Some("game/manifest.json".to_string()),
            }],
            outcome: PluginOutcomeV1::Success {
                output: Box::new(PluginOutputV1::Inspect { package }),
            },
        };

        assert_eq!(response.validate_for(&request, &plugin), Ok(()));

        let wrong_request = PluginRequestV1::new(PluginCallV1::Discover(PluginDiscoverInputV1 {
            project_root: ".".to_string(),
        }));
        assert!(matches!(
            response.validate_for(&wrong_request, &plugin),
            Err(PluginProtocolError::DiagnosticOperationMismatch { .. })
        ));
    }

    #[test]
    fn failed_response_requires_an_error_diagnostic() {
        let plugin = plugin_id();
        let request = PluginRequestV1::new(PluginCallV1::Discover(PluginDiscoverInputV1 {
            project_root: ".".to_string(),
        }));
        let response = PluginResponseV1 {
            schema_version: PLUGIN_PROTOCOL_SCHEMA_VERSION,
            diagnostics: Vec::new(),
            outcome: PluginOutcomeV1::Failure,
        };

        assert_eq!(
            response.validate_for(&request, &plugin),
            Err(PluginProtocolError::FailureWithoutErrorDiagnostic)
        );
    }
}
