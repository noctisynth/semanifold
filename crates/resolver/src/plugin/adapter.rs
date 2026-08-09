use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;

use camino::{Utf8Path, Utf8PathBuf};
use semifold_core::{
    DependencyKind, DependencySource, EcosystemId, EditSource, FileEdit, FileEditExpectation,
    FileHash, PackageId, PackageSnapshot, SharedVersionEdit, VersionSource, VersionSourceId,
};
use semver::Version;
use sha2::{Digest, Sha256};

use crate::adapter::{
    AdapterError, EcosystemAdapter, EcosystemPlanInput, ManifestDependency, PackageInspection,
    PackageLocation,
};

use super::protocol::{
    PluginCallV1, PluginDependencyKindV1, PluginDependencySourceV1, PluginDiagnosticV1,
    PluginDiscoverInputV1, PluginEditSourceV1, PluginFileEditExpectationV1, PluginFileEditV1,
    PluginInspectInputV1, PluginManifestDependencyV1, PluginOperation, PluginOutcomeV1,
    PluginOutputV1, PluginPackageInspectionV1, PluginPackageLocationV1, PluginPackageSnapshotV1,
    PluginPlanEditsInputV1, PluginRequestV1, PluginSharedVersionEditV1, PluginVersionSourceV1,
};
use super::registry::LoadedPlugin;
use super::runtime::PluginRuntimeError;

const PLUGIN_PROJECT_ROOT: &str = ".";

impl EcosystemAdapter for LoadedPlugin {
    fn ecosystem(&self) -> EcosystemId {
        self.metadata().ecosystem.clone()
    }

    fn encode_version(&self, version: &Version) -> Result<String, AdapterError> {
        Ok(version.to_string())
    }

    fn discover(&self, root: &Utf8Path) -> Result<Vec<PackageInspection>, AdapterError> {
        self.validate_project_root(root)?;
        let output = self.call(PluginCallV1::Discover(PluginDiscoverInputV1 {
            project_root: PLUGIN_PROJECT_ROOT.to_owned(),
        }))?;
        let PluginOutputV1::Discover { packages } = output else {
            return Err(self
                .invalid_output(
                    PluginOperation::Discover,
                    "runtime returned an output for a different operation",
                )
                .into());
        };

        let mut seen = BTreeSet::new();
        let mut inspections = packages
            .into_iter()
            .map(|package| {
                if !seen.insert(package.id.clone()) {
                    return Err(self.invalid_output(
                        PluginOperation::Discover,
                        format!("package id {} was returned more than once", package.id),
                    ));
                }
                self.package_inspection(PluginOperation::Discover, package)
            })
            .collect::<Result<Vec<_>, PluginAdapterError>>()?;
        inspections.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(inspections)
    }

    fn inspect(&self, package: &PackageLocation) -> Result<PackageInspection, AdapterError> {
        self.validate_project_root(&package.project_root)?;
        let path = validate_package_path(&package.path).map_err(|reason| {
            self.invalid_output(
                PluginOperation::Inspect,
                format!("invalid input path: {reason}"),
            )
        })?;
        validate_package_directory(self.project_root(), &path).map_err(|reason| {
            self.invalid_output(
                PluginOperation::Inspect,
                format!("invalid input path: {reason}"),
            )
        })?;
        let output = self.call(PluginCallV1::Inspect(PluginInspectInputV1 {
            project_root: PLUGIN_PROJECT_ROOT.to_owned(),
            package: PluginPackageLocationV1 {
                id: package.id.clone(),
                path: path.to_string(),
            },
        }))?;
        let PluginOutputV1::Inspect { package: output } = output else {
            return Err(self
                .invalid_output(
                    PluginOperation::Inspect,
                    "runtime returned an output for a different operation",
                )
                .into());
        };
        if output.id != package.id {
            return Err(self
                .invalid_output(
                    PluginOperation::Inspect,
                    format!(
                        "inspection returned package id {}, expected {}",
                        output.id, package.id
                    ),
                )
                .into());
        }
        if output.path != path.as_str() {
            return Err(self
                .invalid_output(
                    PluginOperation::Inspect,
                    format!(
                        "inspection returned package path {}, expected {}",
                        output.path, path
                    ),
                )
                .into());
        }
        Ok(self.package_inspection(PluginOperation::Inspect, output)?)
    }

    fn plan_edits(&self, input: EcosystemPlanInput<'_>) -> Result<Vec<FileEdit>, AdapterError> {
        self.validate_project_root(input.project_root)?;
        let ecosystem = self.ecosystem();
        let mut workspace_ids = BTreeSet::new();
        let mut workspace_packages = input
            .workspace_packages
            .iter()
            .map(|package| {
                if package.ecosystem != ecosystem {
                    return Err(self.invalid_output(
                        PluginOperation::PlanEdits,
                        format!(
                            "workspace package {} belongs to {}, expected {}",
                            package.id, package.ecosystem, ecosystem
                        ),
                    ));
                }
                if !workspace_ids.insert(package.id.clone()) {
                    return Err(self.invalid_output(
                        PluginOperation::PlanEdits,
                        format!("workspace package id {} occurs more than once", package.id),
                    ));
                }
                self.package_snapshot(package)
            })
            .collect::<Result<Vec<_>, PluginAdapterError>>()?;
        workspace_packages.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.path.cmp(&right.path))
        });

        let mut released_packages = input.released_packages.to_vec();
        released_packages.sort();
        released_packages.dedup();
        for package in &released_packages {
            if !workspace_ids.contains(package) {
                return Err(self
                    .invalid_output(
                        PluginOperation::PlanEdits,
                        format!("released package {package} is not in the plugin workspace"),
                    )
                    .into());
            }
            if !input.versions.contains_key(package) {
                return Err(self
                    .invalid_output(
                        PluginOperation::PlanEdits,
                        format!("released package {package} is missing from VersionMap"),
                    )
                    .into());
            }
        }
        let released_ids = released_packages.iter().cloned().collect::<BTreeSet<_>>();

        let output = self.call(PluginCallV1::PlanEdits(PluginPlanEditsInputV1 {
            project_root: PLUGIN_PROJECT_ROOT.to_owned(),
            workspace_packages,
            released_packages,
            versions: input.versions.clone(),
        }))?;
        let PluginOutputV1::PlanEdits { edits } = output else {
            return Err(self
                .invalid_output(
                    PluginOperation::PlanEdits,
                    "runtime returned an output for a different operation",
                )
                .into());
        };
        self.file_edits(edits, &workspace_ids, &released_ids, input.versions)
            .map_err(AdapterError::from)
    }
}

impl LoadedPlugin {
    fn call(&self, call: PluginCallV1) -> Result<PluginOutputV1, PluginAdapterError> {
        let request = PluginRequestV1::new(call);
        let operation = request.operation();
        let response = self
            .execute(&request)
            .map_err(|source| PluginAdapterError::Runtime {
                plugin: self.ecosystem(),
                operation,
                source,
            })?;
        validate_diagnostics(&response.diagnostics).map_err(|reason| {
            self.invalid_output(operation, format!("invalid diagnostic: {reason}"))
        })?;
        match response.outcome {
            PluginOutcomeV1::Success { output } => Ok(*output),
            PluginOutcomeV1::Failure => Err(PluginAdapterError::OperationFailed {
                plugin: self.ecosystem(),
                operation,
                diagnostics: response.diagnostics,
            }),
        }
    }

    fn validate_project_root(&self, root: &Utf8Path) -> Result<(), PluginAdapterError> {
        let canonical =
            fs::canonicalize(root).map_err(|source| PluginAdapterError::ResolveProjectRoot {
                root: root.to_owned(),
                source,
            })?;
        let canonical = Utf8PathBuf::from_path_buf(canonical).map_err(|path| {
            PluginAdapterError::NonUtf8ProjectRoot {
                path: path.display().to_string(),
            }
        })?;
        if canonical != self.project_root() {
            return Err(PluginAdapterError::ProjectRootMismatch {
                plugin: self.ecosystem(),
                expected: self.project_root().to_owned(),
                actual: canonical,
            });
        }
        Ok(())
    }

    fn package_inspection(
        &self,
        operation: PluginOperation,
        package: PluginPackageInspectionV1,
    ) -> Result<PackageInspection, PluginAdapterError> {
        if package.ecosystem != self.metadata().ecosystem {
            return Err(self.invalid_output(
                operation,
                format!(
                    "package {} belongs to {}, expected {}",
                    package.id,
                    package.ecosystem,
                    self.metadata().ecosystem
                ),
            ));
        }
        if package.id.as_str().is_empty() {
            return Err(self.invalid_output(operation, "package id must not be empty"));
        }
        if package.manifest_name.is_empty() {
            return Err(self.invalid_output(
                operation,
                format!("package {} has an empty manifest name", package.id),
            ));
        }
        let path = validate_package_path(Utf8Path::new(&package.path))
            .and_then(|path| {
                validate_package_directory(self.project_root(), &path)?;
                Ok(path)
            })
            .map_err(|reason| self.invalid_output(operation, reason))?;
        let version_source = self.version_source(operation, package.version_source)?;
        let mut dependencies = package
            .dependencies
            .into_iter()
            .map(|dependency| self.manifest_dependency(operation, dependency))
            .collect::<Result<Vec<_>, _>>()?;
        dependencies.sort_by(|left, right| {
            left.manifest_name
                .cmp(&right.manifest_name)
                .then_with(|| {
                    dependency_kind_rank(left.kind).cmp(&dependency_kind_rank(right.kind))
                })
                .then_with(|| left.requirement.cmp(&right.requirement))
        });
        Ok(PackageInspection {
            id: package.id,
            manifest_name: package.manifest_name,
            version: package.version,
            version_source,
            ecosystem: package.ecosystem,
            path,
            publishable: package.publishable,
            dependencies,
        })
    }

    fn version_source(
        &self,
        operation: PluginOperation,
        source: PluginVersionSourceV1,
    ) -> Result<VersionSource, PluginAdapterError> {
        match source {
            PluginVersionSourceV1::PackageManifest => Ok(VersionSource::PackageManifest),
            PluginVersionSourceV1::Shared { manifest, field } => {
                let manifest = validate_file_path(&manifest)
                    .and_then(|path| {
                        validate_existing_file(self.project_root(), &path).map(|_| path)
                    })
                    .map_err(|reason| self.invalid_output(operation, reason))?;
                if field.is_empty() {
                    return Err(self.invalid_output(
                        operation,
                        "shared version source field must not be empty",
                    ));
                }
                Ok(VersionSource::Shared {
                    source: VersionSourceId { manifest, field },
                })
            }
        }
    }

    fn manifest_dependency(
        &self,
        operation: PluginOperation,
        dependency: PluginManifestDependencyV1,
    ) -> Result<ManifestDependency, PluginAdapterError> {
        if dependency.manifest_name.is_empty() {
            return Err(
                self.invalid_output(operation, "dependency manifest name must not be empty")
            );
        }
        Ok(ManifestDependency {
            manifest_name: dependency.manifest_name,
            kind: dependency_kind(dependency.kind),
            requirement: dependency.requirement,
        })
    }

    fn package_snapshot(
        &self,
        package: &PackageSnapshot,
    ) -> Result<PluginPackageSnapshotV1, PluginAdapterError> {
        let path = validate_package_path(&package.path).map_err(|reason| {
            self.invalid_output(
                PluginOperation::PlanEdits,
                format!("invalid workspace package path: {reason}"),
            )
        })?;
        validate_package_directory(self.project_root(), &path)
            .map_err(|reason| self.invalid_output(PluginOperation::PlanEdits, reason))?;
        let version_source = match &package.version_source {
            VersionSource::PackageManifest => PluginVersionSourceV1::PackageManifest,
            VersionSource::Shared { source } => {
                let manifest = validate_file_path(source.manifest.as_str())
                    .map_err(|reason| self.invalid_output(PluginOperation::PlanEdits, reason))?;
                validate_existing_file(self.project_root(), &manifest)
                    .map_err(|reason| self.invalid_output(PluginOperation::PlanEdits, reason))?;
                if source.field.is_empty() {
                    return Err(self.invalid_output(
                        PluginOperation::PlanEdits,
                        "shared version source field must not be empty",
                    ));
                }
                PluginVersionSourceV1::Shared {
                    manifest: manifest.to_string(),
                    field: source.field.clone(),
                }
            }
        };
        let mut dependencies = package
            .dependencies
            .iter()
            .map(|dependency| super::protocol::PluginDependencyV1 {
                package: dependency.package.clone(),
                kind: plugin_dependency_kind(dependency.kind),
                requirement: dependency.requirement.clone(),
                source: plugin_dependency_source(dependency.source),
            })
            .collect::<Vec<_>>();
        dependencies.sort_by(|left, right| {
            left.package
                .cmp(&right.package)
                .then_with(|| {
                    plugin_dependency_kind_rank(left.kind)
                        .cmp(&plugin_dependency_kind_rank(right.kind))
                })
                .then_with(|| {
                    plugin_dependency_source_rank(left.source)
                        .cmp(&plugin_dependency_source_rank(right.source))
                })
                .then_with(|| left.requirement.cmp(&right.requirement))
        });
        Ok(PluginPackageSnapshotV1 {
            id: package.id.clone(),
            manifest_name: package.manifest_name.clone(),
            version: package.version.clone(),
            version_source,
            ecosystem: package.ecosystem.clone(),
            path: path.to_string(),
            publishable: package.publishable,
            dependencies,
        })
    }

    fn file_edits(
        &self,
        edits: Vec<PluginFileEditV1>,
        workspace_ids: &BTreeSet<PackageId>,
        released_ids: &BTreeSet<PackageId>,
        versions: &BTreeMap<PackageId, Version>,
    ) -> Result<Vec<FileEdit>, PluginAdapterError> {
        let mut paths = BTreeSet::new();
        let mut converted = edits
            .into_iter()
            .map(|edit| {
                let path = validate_file_path(&edit.path)
                    .map_err(|reason| self.invalid_output(PluginOperation::PlanEdits, reason))?;
                if !paths.insert(path.clone()) {
                    return Err(self.invalid_output(
                        PluginOperation::PlanEdits,
                        format!("file edit target {path} was returned more than once"),
                    ));
                }
                let expected = match edit.expected {
                    PluginFileEditExpectationV1::Existing { sha256 } => {
                        let expected = FileHash::from_sha256(&sha256).map_err(|source| {
                            self.invalid_output(PluginOperation::PlanEdits, source.to_string())
                        })?;
                        let actual =
                            hash_existing_file(self.project_root(), &path).map_err(|reason| {
                                self.invalid_output(PluginOperation::PlanEdits, reason)
                            })?;
                        if actual != expected {
                            return Err(self.invalid_output(
                                PluginOperation::PlanEdits,
                                format!(
                                    "file edit hash mismatch for {path}: expected {sha256}, got {}",
                                    actual.as_str()
                                ),
                            ));
                        }
                        FileEditExpectation::Existing { hash: actual }
                    }
                    PluginFileEditExpectationV1::Missing => {
                        validate_missing_file(self.project_root(), &path).map_err(|reason| {
                            self.invalid_output(PluginOperation::PlanEdits, reason)
                        })?;
                        FileEditExpectation::Missing
                    }
                };
                let source =
                    self.edit_source(edit.source, workspace_ids, released_ids, versions)?;
                Ok(FileEdit {
                    path,
                    expected,
                    new_content: edit.new_content,
                    source,
                })
            })
            .collect::<Result<Vec<_>, PluginAdapterError>>()?;
        converted.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.source.cmp(&right.source))
        });
        Ok(converted)
    }

    fn edit_source(
        &self,
        source: PluginEditSourceV1,
        workspace_ids: &BTreeSet<PackageId>,
        released_ids: &BTreeSet<PackageId>,
        versions: &BTreeMap<PackageId, Version>,
    ) -> Result<EditSource, PluginAdapterError> {
        match source {
            PluginEditSourceV1::PackageVersion { package } => {
                validate_released_package(self, &package, released_ids)?;
                Ok(EditSource::PackageVersion { package })
            }
            PluginEditSourceV1::DependencyVersion {
                package,
                dependency,
            } => {
                validate_released_package(self, &package, released_ids)?;
                validate_version_package(self, &dependency, versions)?;
                Ok(EditSource::DependencyVersion {
                    package,
                    dependency,
                })
            }
            PluginEditSourceV1::WorkspaceDependencies { mut dependencies } => {
                normalize_version_packages(self, &mut dependencies, versions)?;
                Ok(EditSource::WorkspaceDependencies { dependencies })
            }
            PluginEditSourceV1::WorkspaceManifest {
                shared_versions,
                mut dependencies,
            } => {
                normalize_version_packages(self, &mut dependencies, versions)?;
                let mut shared_versions = shared_versions
                    .into_iter()
                    .map(|shared| self.shared_version_edit(shared, workspace_ids))
                    .collect::<Result<Vec<_>, _>>()?;
                shared_versions.sort();
                shared_versions.dedup();
                Ok(EditSource::WorkspaceManifest {
                    shared_versions,
                    dependencies,
                })
            }
        }
    }

    fn shared_version_edit(
        &self,
        shared: PluginSharedVersionEditV1,
        workspace_ids: &BTreeSet<PackageId>,
    ) -> Result<SharedVersionEdit, PluginAdapterError> {
        let manifest = validate_file_path(&shared.manifest)
            .and_then(|path| validate_existing_file(self.project_root(), &path).map(|_| path))
            .map_err(|reason| self.invalid_output(PluginOperation::PlanEdits, reason))?;
        if shared.field.is_empty() {
            return Err(self.invalid_output(
                PluginOperation::PlanEdits,
                "shared version edit field must not be empty",
            ));
        }
        let mut packages = shared.packages;
        packages.sort();
        packages.dedup();
        for package in &packages {
            if !workspace_ids.contains(package) {
                return Err(self.invalid_output(
                    PluginOperation::PlanEdits,
                    format!("shared version edit references unknown package {package}"),
                ));
            }
        }
        Ok(SharedVersionEdit {
            source: VersionSourceId {
                manifest,
                field: shared.field,
            },
            packages,
        })
    }

    fn invalid_output(
        &self,
        operation: PluginOperation,
        reason: impl Into<String>,
    ) -> PluginAdapterError {
        PluginAdapterError::InvalidOutput {
            plugin: self.ecosystem(),
            operation,
            reason: reason.into(),
        }
    }
}

fn validate_diagnostics(diagnostics: &[PluginDiagnosticV1]) -> Result<(), String> {
    for diagnostic in diagnostics {
        if diagnostic.code.is_empty() {
            return Err("diagnostic code must not be empty".to_owned());
        }
        if diagnostic.message.is_empty() {
            return Err(format!(
                "diagnostic {} has an empty message",
                diagnostic.code
            ));
        }
        if let Some(path) = &diagnostic.path {
            validate_file_path(path)?;
        }
    }
    Ok(())
}

fn validate_package_path(path: &Utf8Path) -> Result<Utf8PathBuf, String> {
    if path == Utf8Path::new(".") {
        return Ok(path.to_owned());
    }
    validate_file_path(path.as_str())
}

fn validate_file_path(path: &str) -> Result<Utf8PathBuf, String> {
    let invalid = path.is_empty()
        || path.contains('\\')
        || path.starts_with('/')
        || path.ends_with('/')
        || path.split('/').any(|segment| {
            segment.is_empty()
                || segment == "."
                || segment == ".."
                || is_windows_drive_segment(segment)
        });
    if invalid {
        Err(format!(
            "path must be a normalized project-relative UTF-8 path: {path}"
        ))
    } else {
        Ok(Utf8PathBuf::from(path))
    }
}

fn is_windows_drive_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn validate_package_directory(root: &Utf8Path, path: &Utf8Path) -> Result<(), String> {
    let target = root.join(path);
    let canonical = canonical_utf8(&target)?;
    if !canonical.starts_with(root) {
        return Err(format!(
            "package path resolves outside the project root: {path}"
        ));
    }
    if !canonical.is_dir() {
        return Err(format!("package path is not a directory: {path}"));
    }
    Ok(())
}

fn validate_existing_file(root: &Utf8Path, path: &Utf8Path) -> Result<Utf8PathBuf, String> {
    let target = root.join(path);
    let canonical = canonical_utf8(&target)?;
    if !canonical.starts_with(root) {
        return Err(format!(
            "file path resolves outside the project root: {path}"
        ));
    }
    if !canonical.is_file() {
        return Err(format!("file path is not a regular file: {path}"));
    }
    Ok(canonical)
}

fn hash_existing_file(root: &Utf8Path, path: &Utf8Path) -> Result<FileHash, String> {
    let canonical = validate_existing_file(root, path)?;
    let mut file =
        File::open(&canonical).map_err(|source| format!("failed to read {path}: {source}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|source| format!("failed to read {path}: {source}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let value = format!("{:x}", hasher.finalize());
    FileHash::from_sha256(value)
        .map_err(|source| format!("failed to construct the validated hash for {path}: {source}"))
}

fn validate_missing_file(root: &Utf8Path, path: &Utf8Path) -> Result<(), String> {
    let target = root.join(path);
    match fs::symlink_metadata(&target) {
        Ok(_) => {
            return Err(format!(
                "file edit expects a missing target, but {path} exists"
            ));
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => return Err(format!("failed to inspect {path}: {source}")),
    }
    let parent = target
        .parent()
        .ok_or_else(|| format!("file edit target has no parent directory: {path}"))?;
    let canonical_parent = canonical_utf8(parent)?;
    if !canonical_parent.starts_with(root) {
        return Err(format!(
            "file edit parent resolves outside the project root: {path}"
        ));
    }
    if !canonical_parent.is_dir() {
        return Err(format!("file edit parent is not a directory: {path}"));
    }
    Ok(())
}

fn canonical_utf8(path: &Utf8Path) -> Result<Utf8PathBuf, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|source| format!("failed to resolve path {path}: {source}"))?;
    Utf8PathBuf::from_path_buf(canonical)
        .map_err(|path| format!("resolved path is not UTF-8: {}", path.display()))
}

fn validate_released_package(
    plugin: &LoadedPlugin,
    package: &PackageId,
    released_ids: &BTreeSet<PackageId>,
) -> Result<(), PluginAdapterError> {
    if released_ids.contains(package) {
        Ok(())
    } else {
        Err(plugin.invalid_output(
            PluginOperation::PlanEdits,
            format!("file edit source references unreleased package {package}"),
        ))
    }
}

fn validate_version_package(
    plugin: &LoadedPlugin,
    package: &PackageId,
    versions: &BTreeMap<PackageId, Version>,
) -> Result<(), PluginAdapterError> {
    if versions.contains_key(package) {
        Ok(())
    } else {
        Err(plugin.invalid_output(
            PluginOperation::PlanEdits,
            format!("file edit source references package {package} missing from VersionMap"),
        ))
    }
}

fn normalize_version_packages(
    plugin: &LoadedPlugin,
    packages: &mut Vec<PackageId>,
    versions: &BTreeMap<PackageId, Version>,
) -> Result<(), PluginAdapterError> {
    packages.sort();
    packages.dedup();
    for package in packages {
        validate_version_package(plugin, package, versions)?;
    }
    Ok(())
}

const fn dependency_kind(kind: PluginDependencyKindV1) -> DependencyKind {
    match kind {
        PluginDependencyKindV1::Unspecified => DependencyKind::Unspecified,
        PluginDependencyKindV1::Runtime => DependencyKind::Runtime,
        PluginDependencyKindV1::Development => DependencyKind::Development,
        PluginDependencyKindV1::Build => DependencyKind::Build,
        PluginDependencyKindV1::Optional => DependencyKind::Optional,
        PluginDependencyKindV1::Peer => DependencyKind::Peer,
    }
}

const fn plugin_dependency_kind(kind: DependencyKind) -> PluginDependencyKindV1 {
    match kind {
        DependencyKind::Unspecified => PluginDependencyKindV1::Unspecified,
        DependencyKind::Runtime => PluginDependencyKindV1::Runtime,
        DependencyKind::Development => PluginDependencyKindV1::Development,
        DependencyKind::Build => PluginDependencyKindV1::Build,
        DependencyKind::Optional => PluginDependencyKindV1::Optional,
        DependencyKind::Peer => PluginDependencyKindV1::Peer,
    }
}

const fn plugin_dependency_source(source: DependencySource) -> PluginDependencySourceV1 {
    match source {
        DependencySource::Manifest => PluginDependencySourceV1::Manifest,
        DependencySource::Config => PluginDependencySourceV1::Config,
    }
}

const fn dependency_kind_rank(kind: DependencyKind) -> u8 {
    match kind {
        DependencyKind::Unspecified => 0,
        DependencyKind::Runtime => 1,
        DependencyKind::Development => 2,
        DependencyKind::Build => 3,
        DependencyKind::Optional => 4,
        DependencyKind::Peer => 5,
    }
}

const fn plugin_dependency_kind_rank(kind: PluginDependencyKindV1) -> u8 {
    match kind {
        PluginDependencyKindV1::Unspecified => 0,
        PluginDependencyKindV1::Runtime => 1,
        PluginDependencyKindV1::Development => 2,
        PluginDependencyKindV1::Build => 3,
        PluginDependencyKindV1::Optional => 4,
        PluginDependencyKindV1::Peer => 5,
    }
}

const fn plugin_dependency_source_rank(source: PluginDependencySourceV1) -> u8 {
    match source {
        PluginDependencySourceV1::Manifest => 0,
        PluginDependencySourceV1::Config => 1,
    }
}

/// Failure raised while converting an authenticated plugin response into adapter domain data.
#[derive(Debug, thiserror::Error)]
pub enum PluginAdapterError {
    #[error("failed to resolve plugin adapter project root `{root}`: {source}")]
    ResolveProjectRoot {
        root: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("plugin adapter project root is not UTF-8: `{path}`")]
    NonUtf8ProjectRoot { path: String },
    #[error("plugin {plugin} is bound to project root `{expected}`, but received `{actual}`")]
    ProjectRootMismatch {
        plugin: EcosystemId,
        expected: Utf8PathBuf,
        actual: Utf8PathBuf,
    },
    #[error("plugin {plugin} failed while executing {operation:?}: {source}")]
    Runtime {
        plugin: EcosystemId,
        operation: PluginOperation,
        #[source]
        source: PluginRuntimeError,
    },
    #[error("plugin {plugin} reported failure while executing {operation:?}")]
    OperationFailed {
        plugin: EcosystemId,
        operation: PluginOperation,
        diagnostics: Vec<PluginDiagnosticV1>,
    },
    #[error("plugin {plugin} returned invalid {operation:?} output: {reason}")]
    InvalidOutput {
        plugin: EcosystemId,
        operation: PluginOperation,
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    use semifold_core::Dependency;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::plugin::protocol::PluginDiagnosticSeverityV1;
    use crate::plugin::registry::{PluginDefinition, PluginRegistry};
    use crate::plugin::runtime::BoaPluginRuntime;

    fn fixture_root(test: &str) -> Utf8PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "semifold-plugin-adapter-{}-{test}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Utf8PathBuf::from_path_buf(root).unwrap()
    }

    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn load_plugin(root: &Utf8Path, source: &str) -> LoadedPlugin {
        let plugin_path = Utf8Path::new("plugins/example.js");
        fs::create_dir_all(root.join("plugins")).unwrap();
        fs::write(root.join(plugin_path), source).unwrap();
        let definition =
            PluginDefinition::new(EcosystemId::new("com.example.game").unwrap(), plugin_path)
                .unwrap()
                .with_sha256(digest(source.as_bytes()))
                .unwrap();
        let registry =
            PluginRegistry::load(root, [definition], BoaPluginRuntime::default()).unwrap();
        registry
            .get(&EcosystemId::new("com.example.game").unwrap())
            .unwrap()
            .clone()
    }

    fn successful_source(manifest_hash: &str, dependency_hash: &str) -> String {
        format!(
            r#"
            export const metadata = {{
                "schema-version": 1,
                ecosystem: "com.example.game",
                "plugin-version": "1.0.0",
                operations: ["discover", "inspect", "plan-edits"],
                "read-patterns": ["game/*.json"]
            }};

            const packageInspection = (id, path) => ({{
                id,
                "manifest-name": "game",
                version: "1.0.0",
                "version-source": {{ kind: "package-manifest" }},
                ecosystem: "com.example.game",
                path,
                publishable: true,
                dependencies: [{{
                    "manifest-name": "engine",
                    kind: "runtime",
                    requirement: "^2.0.0"
                }}]
            }});

            export default function(request) {{
                let output;
                if (request.operation === "discover") {{
                    output = {{ packages: [packageInspection("game", "game")] }};
                }} else if (request.operation === "inspect") {{
                    const location = request.input.package;
                    output = {{ package: packageInspection(location.id, location.path) }};
                }} else {{
                    output = {{ edits: [
                        {{
                            path: "game/dependency.json",
                            expected: {{ kind: "existing", sha256: "{dependency_hash}" }},
                            "new-content": "{{\"version\":\"2.1.0\"}}\n",
                            source: {{
                                kind: "dependency-version",
                                package: "game",
                                dependency: "engine"
                            }}
                        }},
                        {{
                            path: "game/manifest.json",
                            expected: {{ kind: "existing", sha256: "{manifest_hash}" }},
                            "new-content": "{{\"version\":\"1.1.0\"}}\n",
                            source: {{ kind: "package-version", package: "game" }}
                        }}
                    ] }};
                }}
                return {{
                    "schema-version": 1,
                    diagnostics: [],
                    status: "success",
                    output: {{ operation: request.operation, output }}
                }};
            }};
            "#
        )
    }

    fn package_snapshot(inspection: &PackageInspection) -> PackageSnapshot {
        PackageSnapshot {
            id: inspection.id.clone(),
            manifest_name: inspection.manifest_name.clone(),
            version: inspection.version.clone(),
            version_source: inspection.version_source.clone(),
            ecosystem: inspection.ecosystem.clone(),
            path: inspection.path.clone(),
            publishable: inspection.publishable,
            dependencies: vec![Dependency {
                package: PackageId::new("engine"),
                kind: DependencyKind::Runtime,
                requirement: Some("^2.0.0".to_owned()),
                source: DependencySource::Manifest,
            }],
        }
    }

    #[test]
    fn loaded_plugin_implements_the_complete_adapter_contract_without_writing_files() {
        let root = fixture_root("contract");
        fs::create_dir_all(root.join("game")).unwrap();
        let manifest = b"{\"version\":\"1.0.0\"}\n";
        let dependency = b"{\"version\":\"2.0.0\"}\n";
        fs::write(root.join("game/manifest.json"), manifest).unwrap();
        fs::write(root.join("game/dependency.json"), dependency).unwrap();
        let plugin = load_plugin(
            &root,
            &successful_source(&digest(manifest), &digest(dependency)),
        );
        let adapter: Box<dyn EcosystemAdapter> = Box::new(plugin);

        assert_eq!(
            adapter.encode_version(&Version::new(1, 2, 3)).unwrap(),
            "1.2.3"
        );
        let discovered = adapter.discover(&root).unwrap();
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].id, PackageId::new("game"));
        assert_eq!(discovered[0].dependencies[0].manifest_name, "engine");
        let inspected = adapter
            .inspect(&PackageLocation {
                id: PackageId::new("configured-game"),
                project_root: root.clone(),
                path: "game".into(),
            })
            .unwrap();
        assert_eq!(inspected.id, PackageId::new("configured-game"));

        let snapshot = package_snapshot(&discovered[0]);
        let versions = BTreeMap::from([
            (PackageId::new("game"), Version::new(1, 1, 0)),
            (PackageId::new("engine"), Version::new(2, 1, 0)),
        ]);
        let edits = adapter
            .plan_edits(EcosystemPlanInput {
                project_root: &root,
                workspace_packages: std::slice::from_ref(&snapshot),
                released_packages: std::slice::from_ref(&snapshot.id),
                versions: &versions,
            })
            .unwrap();
        let repeated = adapter
            .plan_edits(EcosystemPlanInput {
                project_root: &root,
                workspace_packages: std::slice::from_ref(&snapshot),
                released_packages: std::slice::from_ref(&snapshot.id),
                versions: &versions,
            })
            .unwrap();

        assert_eq!(repeated, edits);
        assert_eq!(
            edits
                .iter()
                .map(|edit| edit.path.as_str())
                .collect::<Vec<_>>(),
            vec!["game/dependency.json", "game/manifest.json"]
        );
        assert!(matches!(
            &edits[0].source,
            EditSource::DependencyVersion { package, dependency }
                if package == &PackageId::new("game")
                    && dependency == &PackageId::new("engine")
        ));
        assert_eq!(fs::read(root.join("game/manifest.json")).unwrap(), manifest);
        assert_eq!(
            fs::read(root.join("game/dependency.json")).unwrap(),
            dependency
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_calls_for_a_different_project_root() {
        let root = fixture_root("bound-root");
        let other = fixture_root("other-root");
        fs::create_dir_all(root.join("game")).unwrap();
        let manifest = b"{\"version\":\"1.0.0\"}\n";
        let dependency = b"{\"version\":\"2.0.0\"}\n";
        fs::write(root.join("game/manifest.json"), manifest).unwrap();
        fs::write(root.join("game/dependency.json"), dependency).unwrap();
        let plugin = load_plugin(
            &root,
            &successful_source(&digest(manifest), &digest(dependency)),
        );

        assert!(matches!(
            plugin.discover(&other),
            Err(AdapterError::Plugin(
                PluginAdapterError::ProjectRootMismatch { .. }
            ))
        ));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(other).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_existing_and_missing_edit_targets_through_outside_symlinks() {
        use std::os::unix::fs::symlink;

        let root = fixture_root("edit-symlink-root");
        let outside = fixture_root("edit-symlink-outside");
        fs::write(outside.join("manifest.json"), "outside").unwrap();
        symlink(outside.join("manifest.json"), root.join("manifest.json")).unwrap();
        symlink(&outside, root.join("outside-dir")).unwrap();

        assert!(validate_existing_file(&root, Utf8Path::new("manifest.json")).is_err());
        assert!(validate_missing_file(&root, Utf8Path::new("outside-dir/new.json")).is_err());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn rejects_untrusted_package_paths_and_duplicate_discovery_identities() {
        assert!(validate_package_path(Utf8Path::new("../outside")).is_err());
        let root = fixture_root("invalid-discovery");
        fs::create_dir_all(root.join("game")).unwrap();
        let source = r#"
            export const metadata = {
                "schema-version": 1,
                ecosystem: "com.example.game",
                "plugin-version": "1.0.0",
                operations: ["discover", "inspect", "plan-edits"]
            };
            export default function(request) {
                const discoveredPackage = {
                    id: "game",
                    "manifest-name": "game",
                    version: "1.0.0",
                    "version-source": { kind: "package-manifest" },
                    ecosystem: "com.example.game",
                    path: "game",
                    publishable: true,
                    dependencies: []
                };
                return {
                    "schema-version": 1,
                    diagnostics: [],
                    status: "success",
                    output: {
                        operation: request.operation,
                        output: { packages: [discoveredPackage, discoveredPackage] }
                    }
                };
            }
        "#;
        let plugin = load_plugin(&root, source);

        assert!(matches!(
            plugin.discover(&root),
            Err(AdapterError::Plugin(PluginAdapterError::InvalidOutput {
                operation: PluginOperation::Discover,
                ..
            }))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_stale_edit_hashes_and_keeps_the_workspace_unchanged() {
        let root = fixture_root("stale-hash");
        fs::create_dir_all(root.join("game")).unwrap();
        let manifest = b"{\"version\":\"1.0.0\"}\n";
        let dependency = b"{\"version\":\"2.0.0\"}\n";
        fs::write(root.join("game/manifest.json"), manifest).unwrap();
        fs::write(root.join("game/dependency.json"), dependency).unwrap();
        let plugin = load_plugin(
            &root,
            &successful_source(&"0".repeat(64), &digest(dependency)),
        );
        let inspection = plugin.discover(&root).unwrap().remove(0);
        let snapshot = package_snapshot(&inspection);
        let versions = BTreeMap::from([
            (PackageId::new("game"), Version::new(1, 1, 0)),
            (PackageId::new("engine"), Version::new(2, 1, 0)),
        ]);

        assert!(matches!(
            plugin.plan_edits(EcosystemPlanInput {
                project_root: &root,
                workspace_packages: std::slice::from_ref(&snapshot),
                released_packages: std::slice::from_ref(&snapshot.id),
                versions: &versions,
            }),
            Err(AdapterError::Plugin(PluginAdapterError::InvalidOutput {
                operation: PluginOperation::PlanEdits,
                ..
            }))
        ));
        assert_eq!(fs::read(root.join("game/manifest.json")).unwrap(), manifest);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preserves_structured_failure_diagnostics_at_the_adapter_boundary() {
        let root = fixture_root("diagnostics");
        let source = r#"
            export const metadata = {
                "schema-version": 1,
                ecosystem: "com.example.game",
                "plugin-version": "1.0.0",
                operations: ["discover", "inspect", "plan-edits"]
            };
            export default function(request) {
                return {
                    "schema-version": 1,
                    diagnostics: [{
                        plugin: "com.example.game",
                        operation: request.operation,
                        severity: "error",
                        code: "manifest-invalid",
                        message: "The manifest is invalid.",
                        path: "game/manifest.json"
                    }],
                    status: "failure"
                };
            }
        "#;
        let plugin = load_plugin(&root, source);

        let error = plugin.discover(&root).unwrap_err();
        let AdapterError::Plugin(PluginAdapterError::OperationFailed { diagnostics, .. }) = error
        else {
            panic!("expected a structured plugin operation failure");
        };
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "manifest-invalid");
        assert_eq!(diagnostics[0].severity, PluginDiagnosticSeverityV1::Error);
        fs::remove_dir_all(root).unwrap();
    }
}
