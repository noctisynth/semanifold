use std::collections::BTreeMap;

use semifold_core::{PackageId, ReleaseContext};
use semver::Version;
use serde::Serialize;

use crate::{
    PublishPlan,
    publish_plan::{CommandPhase, PublishSkipReason},
    publisher::{PublishFailureStage, PublishReport, PublishStatus},
};

pub const WORKFLOW_OUTPUT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowExecutionMode {
    Apply,
    DryRun,
}

impl WorkflowExecutionMode {
    #[must_use]
    pub const fn is_dry_run(self) -> bool {
        matches!(self, Self::DryRun)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct VersionWorkflowOutput {
    pub schema_version: u32,
    pub dry_run: bool,
    pub plan_fingerprint: String,
    pub release_branch: String,
    pub packages: BTreeMap<PackageId, VersionWorkflowPackage>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct VersionWorkflowPackage {
    pub current_version: Version,
    pub next_version: Version,
}

impl VersionWorkflowOutput {
    #[must_use]
    pub fn from_release(
        release: &ReleaseContext,
        release_branch: String,
        mode: WorkflowExecutionMode,
    ) -> Self {
        let packages = release
            .plan
            .packages
            .iter()
            .map(|(id, package)| {
                (
                    id.clone(),
                    VersionWorkflowPackage {
                        current_version: package.current_version.clone(),
                        next_version: package.next_version.clone(),
                    },
                )
            })
            .collect();
        Self {
            schema_version: WORKFLOW_OUTPUT_SCHEMA_VERSION,
            dry_run: mode.is_dry_run(),
            plan_fingerprint: release.plan.fingerprint.clone(),
            release_branch,
            packages,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PublishWorkflowOutput {
    pub schema_version: u32,
    pub dry_run: bool,
    pub packages: Vec<PublishWorkflowPackage>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PublishWorkflowPackage {
    pub package: PackageId,
    pub version: Version,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_stage: Option<&'static str>,
}

impl PublishWorkflowOutput {
    #[must_use]
    pub fn from_plan_and_report(
        plan: &PublishPlan,
        report: &PublishReport,
        mode: WorkflowExecutionMode,
    ) -> Self {
        let reports = report
            .packages
            .iter()
            .map(|package| (&package.package, package))
            .collect::<BTreeMap<_, _>>();
        let packages = plan
            .packages
            .iter()
            .map(|planned| {
                let (status, skip_reason, failure_stage) = reports
                    .get(&planned.context.package.id)
                    .map_or(("not-started", None, None), |report| {
                        status_fields(&report.status)
                    });
                PublishWorkflowPackage {
                    package: planned.context.package.id.clone(),
                    version: planned.context.package.version.clone(),
                    status,
                    skip_reason,
                    failure_stage,
                }
            })
            .collect();
        Self {
            schema_version: WORKFLOW_OUTPUT_SCHEMA_VERSION,
            dry_run: mode.is_dry_run(),
            packages,
        }
    }
}

fn status_fields(
    status: &PublishStatus,
) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    match status {
        PublishStatus::Succeeded => ("succeeded", None, None),
        PublishStatus::Skipped(reason) => ("skipped", Some(skip_reason(*reason)), None),
        PublishStatus::Failed(stage) => ("failed", None, Some(failure_stage(*stage))),
        PublishStatus::NotStarted => ("not-started", None, None),
    }
}

const fn skip_reason(reason: PublishSkipReason) -> &'static str {
    match reason {
        PublishSkipReason::Private => "private",
        PublishSkipReason::MissingChangelog => "missing-changelog",
        PublishSkipReason::RegistryVersionExists => "registry-version-exists",
    }
}

const fn failure_stage(stage: PublishFailureStage) -> &'static str {
    match stage {
        PublishFailureStage::Preflight => "preflight",
        PublishFailureStage::Command(CommandPhase::Prepublish) => "prepublish-command",
        PublishFailureStage::Command(CommandPhase::Publish) => "publish-command",
        PublishFailureStage::Command(CommandPhase::PostVersion) => "post-version-command",
        PublishFailureStage::ForgeRelease => "forge-release",
        PublishFailureStage::AssetUpload => "asset-upload",
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use semifold_core::{
        BumpLevel, ChangesetId, EcosystemId, PackageRelease, ReleasePlan, ReleaseReason, VersionMap,
    };

    use super::*;
    use crate::{
        publish_plan::{
            CommandSpec, PackagePublish, PublishContext, PublishPackageContext, StdioPolicy,
        },
        publisher::{ForgeDisposition, PackagePublishReport},
    };

    fn release_context() -> ReleaseContext {
        let package = PackageRelease {
            id: PackageId::new("core"),
            ecosystem: EcosystemId::RUST,
            current_version: Version::new(1, 0, 0),
            next_version: Version::new(1, 1, 0),
            bump: BumpLevel::Minor,
            reasons: vec![ReleaseReason::Changeset {
                changeset: ChangesetId::new("bright-dog"),
            }],
        };
        let plan = ReleasePlan::new(
            vec![package],
            VersionMap::from([(PackageId::new("core"), Version::new(1, 1, 0))]),
            vec![PackageId::new("core")],
            vec![ChangesetId::new("bright-dog")],
            Vec::new(),
            Vec::new(),
        )
        .expect("workflow output fixture is a valid release plan");
        ReleaseContext::from_plan(&plan)
    }

    fn publish_plan() -> PublishPlan {
        PublishPlan {
            project_root: Utf8PathBuf::from("/secret/project"),
            packages: vec![PackagePublish {
                context: PublishContext {
                    package: PublishPackageContext {
                        id: PackageId::new("core"),
                        name: "core-native".to_string(),
                        ecosystem: EcosystemId::RUST,
                        version: Version::new(1, 1, 0),
                        tag: "core-v1.1.0".to_string(),
                        path: Utf8PathBuf::from("crates/core"),
                        private: false,
                    },
                    repository: None,
                    ci: None,
                },
                preflight: None,
                commands: vec![CommandSpec {
                    executable: "secret-command".to_string(),
                    args: vec!["secret-argument".to_string()],
                    environment: BTreeMap::from([(
                        "TOKEN".to_string(),
                        "secret-token".to_string(),
                    )]),
                    working_directory: Utf8PathBuf::from("/secret/project/crates/core"),
                    phase: CommandPhase::Publish,
                    stdout: StdioPolicy::Null,
                    stderr: StdioPolicy::Null,
                    run_in_dry_run: false,
                }],
                assets: Vec::new(),
                forge: None,
                skip_reason: None,
            }],
        }
    }

    #[test]
    fn version_schema_is_stable_and_allowlisted() {
        let output = VersionWorkflowOutput::from_release(
            &release_context(),
            "release/abc".to_string(),
            WorkflowExecutionMode::Apply,
        );
        let value = serde_json::to_value(output).expect("workflow output serializes");
        assert_eq!(value["schema-version"], 1);
        assert_eq!(value["release-branch"], "release/abc");
        assert_eq!(value["packages"]["core"]["next-version"], "1.1.0");
        assert_eq!(
            value.as_object().map(|object| object.len()),
            Some(5),
            "new top-level fields require a schema compatibility review"
        );
    }

    #[test]
    fn publish_schema_preserves_recovery_state_without_plan_secrets() {
        let plan = publish_plan();
        let report = PublishReport {
            packages: vec![PackagePublishReport {
                package: PackageId::new("core"),
                status: PublishStatus::Failed(PublishFailureStage::Command(CommandPhase::Publish)),
                commands: Vec::new(),
                forge: ForgeDisposition::NotRequested,
                error: Some("registry rejected package".to_string()),
            }],
        };
        let output = PublishWorkflowOutput::from_plan_and_report(
            &plan,
            &report,
            WorkflowExecutionMode::DryRun,
        );
        let json = serde_json::to_string(&output).expect("workflow output serializes");
        assert!(json.contains("\"schema-version\":1"));
        assert!(json.contains("\"status\":\"failed\""));
        assert!(json.contains("\"failure-stage\":\"publish-command\""));
        assert!(!json.contains("registry rejected package"));
        for secret in [
            "secret-command",
            "secret-argument",
            "secret-token",
            "/secret/project",
            "TOKEN",
        ] {
            assert!(!json.contains(secret), "output leaked {secret}");
        }
    }
}
