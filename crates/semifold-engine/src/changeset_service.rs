use std::{collections::BTreeSet, fs, io};

use camino::Utf8PathBuf;
use semifold_core::{BumpLevel, ChangesetId, FileHash, PackageId};
use semifold_resolver::{changeset::Changeset, error::ResolveError};
use thiserror::Error;

use crate::{ExecutionMode, Project, service::EngineDependencies};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangesetDraft {
    pub name: String,
    pub packages: Vec<ChangesetPackageInput>,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangesetPackageInput {
    pub package: PackageId,
    pub bump: BumpLevel,
    pub tag: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangesetRecord {
    pub id: ChangesetId,
    pub packages: Vec<ChangesetPackageInput>,
    pub summary: String,
    pub path: Utf8PathBuf,
    pub revision: FileHash,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangesetMutationStatus {
    Planned,
    Created,
    Existing,
    Updated,
    Deleted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangesetMutationResult {
    pub status: ChangesetMutationStatus,
    pub changeset: ChangesetRecord,
}

#[derive(Debug, Error)]
pub enum ChangesetCrudError {
    #[error("changeset input is invalid: {0}")]
    Invalid(#[from] ChangesetCreateError),
    #[error("changeset id is invalid: {id}")]
    InvalidId { id: String },
    #[error("changeset draft id {draft} does not match target id {target}")]
    IdMismatch { target: String, draft: String },
    #[error("changeset {id} was not found")]
    NotFound { id: ChangesetId },
    #[error("changeset {id} already exists with different content")]
    Conflict { id: ChangesetId, actual: FileHash },
    #[error("changeset {id} changed after it was read")]
    RevisionMismatch {
        id: ChangesetId,
        expected: FileHash,
        actual: FileHash,
    },
    #[error("failed to read changeset directory {path}")]
    ReadDirectory {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("changeset path is not valid UTF-8: {path:?}")]
    NonUtf8Path { path: std::path::PathBuf },
    #[error("failed to read changeset {path}")]
    Read {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse changeset {path}")]
    Parse {
        path: Utf8PathBuf,
        #[source]
        source: ResolveError,
    },
    #[error("failed to write changeset {path}")]
    Write {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to delete changeset {path}")]
    Delete {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
}

struct PreparedChangeset {
    record: ChangesetRecord,
    content: String,
}

#[derive(Debug, Error)]
pub enum ChangesetCreateError {
    #[error("changeset name is empty")]
    EmptyName,
    #[error("changeset {name} already exists")]
    AlreadyExists { name: String },
    #[error("changeset summary is empty")]
    EmptySummary,
    #[error("changeset contains no packages")]
    EmptyPackages,
    #[error("changeset package {package} is not configured")]
    PackageNotFound { package: PackageId },
    #[error("changeset package {package} is listed more than once")]
    DuplicatePackage { package: PackageId },
    #[error("changeset package {package} has an unchanged bump")]
    UnchangedPackage { package: PackageId },
    #[error("changeset tag {tag} is not configured")]
    TagNotFound { tag: String },
    #[error("failed to write changeset")]
    Write(#[source] ResolveError),
}

pub fn create_changeset(
    project: &Project,
    draft: ChangesetDraft,
) -> Result<ChangesetId, ChangesetCreateError> {
    let prepared = prepare_changeset(project, draft)?;
    if prepared.record.path.exists() {
        return Err(ChangesetCreateError::AlreadyExists {
            name: prepared.record.id.to_string(),
        });
    }
    let mut changeset = changeset_from_record(&prepared.record, &project.changeset_dir);
    changeset.commit().map_err(ChangesetCreateError::Write)?;
    Ok(prepared.record.id)
}

pub fn get_changesets(
    project: &Project,
    id: Option<&str>,
) -> Result<Vec<ChangesetRecord>, ChangesetCrudError> {
    if let Some(id) = id {
        let id = validate_id(id)?;
        let path = project.changeset_dir.join(format!("{id}.md"));
        if !path.is_file() {
            return Err(ChangesetCrudError::NotFound {
                id: ChangesetId::new(id),
            });
        }
        return read_record(project, path).map(|record| vec![record]);
    }

    let entries = fs::read_dir(&project.changeset_dir).map_err(|source| {
        ChangesetCrudError::ReadDirectory {
            path: project.changeset_dir.clone(),
            source,
        }
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| ChangesetCrudError::ReadDirectory {
            path: project.changeset_dir.clone(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        paths.push(
            Utf8PathBuf::from_path_buf(path)
                .map_err(|path| ChangesetCrudError::NonUtf8Path { path })?,
        );
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| read_record(project, path))
        .collect()
}

pub fn create_changeset_idempotent<D: EngineDependencies>(
    dependencies: &D,
    project: &Project,
    draft: ChangesetDraft,
    mode: ExecutionMode,
) -> Result<ChangesetMutationResult, ChangesetCrudError> {
    let prepared = prepare_changeset(project, draft)?;
    if prepared.record.path.exists() {
        return matching_existing(project, &prepared);
    }
    if matches!(mode, ExecutionMode::DryRun) {
        return Ok(ChangesetMutationResult {
            status: ChangesetMutationStatus::Planned,
            changeset: prepared.record,
        });
    }

    match dependencies.write_new_atomic(&prepared.record.path, &prepared.content) {
        Ok(()) => Ok(ChangesetMutationResult {
            status: ChangesetMutationStatus::Created,
            changeset: prepared.record,
        }),
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
            matching_existing(project, &prepared)
        }
        Err(source) => Err(ChangesetCrudError::Write {
            path: prepared.record.path,
            source,
        }),
    }
}

pub fn update_changeset<D: EngineDependencies>(
    dependencies: &D,
    project: &Project,
    id: &str,
    expected_revision: &FileHash,
    draft: ChangesetDraft,
    mode: ExecutionMode,
) -> Result<ChangesetMutationResult, ChangesetCrudError> {
    let id = validate_id(id)?;
    let draft_id = sanitize_name(&draft.name);
    if id != draft_id {
        return Err(ChangesetCrudError::IdMismatch {
            target: id,
            draft: draft_id,
        });
    }
    let prepared = prepare_changeset(project, draft)?;
    let current = read_record(project, prepared.record.path.clone())?;
    verify_revision(&current, expected_revision)?;
    if current.revision == prepared.record.revision {
        return Ok(ChangesetMutationResult {
            status: ChangesetMutationStatus::Existing,
            changeset: current,
        });
    }
    if matches!(mode, ExecutionMode::DryRun) {
        return Ok(ChangesetMutationResult {
            status: ChangesetMutationStatus::Planned,
            changeset: prepared.record,
        });
    }
    dependencies
        .write_atomic(&prepared.record.path, &prepared.content)
        .map_err(|source| ChangesetCrudError::Write {
            path: prepared.record.path.clone(),
            source,
        })?;
    Ok(ChangesetMutationResult {
        status: ChangesetMutationStatus::Updated,
        changeset: prepared.record,
    })
}

pub fn delete_changeset<D: EngineDependencies>(
    dependencies: &D,
    project: &Project,
    id: &str,
    expected_revision: &FileHash,
    mode: ExecutionMode,
) -> Result<ChangesetMutationResult, ChangesetCrudError> {
    let id = validate_id(id)?;
    let path = project.changeset_dir.join(format!("{id}.md"));
    if !path.is_file() {
        return Err(ChangesetCrudError::NotFound {
            id: ChangesetId::new(id),
        });
    }
    let current = read_record(project, path.clone())?;
    verify_revision(&current, expected_revision)?;
    if matches!(mode, ExecutionMode::DryRun) {
        return Ok(ChangesetMutationResult {
            status: ChangesetMutationStatus::Planned,
            changeset: current,
        });
    }
    dependencies
        .remove_file(&path)
        .map_err(|source| ChangesetCrudError::Delete { path, source })?;
    Ok(ChangesetMutationResult {
        status: ChangesetMutationStatus::Deleted,
        changeset: current,
    })
}

fn prepare_changeset(
    project: &Project,
    draft: ChangesetDraft,
) -> Result<PreparedChangeset, ChangesetCreateError> {
    let name = sanitize_name(&draft.name);
    if name.is_empty() {
        return Err(ChangesetCreateError::EmptyName);
    }
    if draft.summary.trim().is_empty() {
        return Err(ChangesetCreateError::EmptySummary);
    }
    if draft.packages.is_empty() {
        return Err(ChangesetCreateError::EmptyPackages);
    }
    let mut seen = BTreeSet::new();
    let mut changeset = Changeset::new(name.clone(), project.changeset_dir.as_std_path());
    for input in draft.packages {
        if !project.config.packages.contains_key(input.package.as_str()) {
            return Err(ChangesetCreateError::PackageNotFound {
                package: input.package,
            });
        }
        if !seen.insert(input.package.clone()) {
            return Err(ChangesetCreateError::DuplicatePackage {
                package: input.package,
            });
        }
        if let Some(tag) = input.tag.as_ref()
            && !project.config.tags.contains_key(tag)
        {
            return Err(ChangesetCreateError::TagNotFound { tag: tag.clone() });
        }
        changeset.add_package(
            input.package.to_string(),
            resolver_bump(input.bump).ok_or_else(|| ChangesetCreateError::UnchangedPackage {
                package: input.package.clone(),
            })?,
            input.tag,
        );
    }
    changeset.summary(draft.summary);
    let content = changeset.render().map_err(ChangesetCreateError::Write)?;
    let record = record_from_changeset(
        &changeset,
        project.changeset_dir.join(format!("{name}.md")),
        &content,
    );
    Ok(PreparedChangeset { record, content })
}

fn validate_id(id: &str) -> Result<String, ChangesetCrudError> {
    if id.is_empty() || sanitize_name(id) != id {
        Err(ChangesetCrudError::InvalidId { id: id.to_string() })
    } else {
        Ok(id.to_string())
    }
}

fn read_record(
    project: &Project,
    path: Utf8PathBuf,
) -> Result<ChangesetRecord, ChangesetCrudError> {
    if !path.is_file() {
        let id = path.file_stem().unwrap_or_default();
        return Err(ChangesetCrudError::NotFound {
            id: ChangesetId::new(id),
        });
    }
    let content = fs::read_to_string(&path).map_err(|source| ChangesetCrudError::Read {
        path: path.clone(),
        source,
    })?;
    let changeset = Changeset::from_file(&project.config, &path.as_std_path().to_path_buf())
        .map_err(|source| ChangesetCrudError::Parse {
            path: path.clone(),
            source,
        })?;
    Ok(record_from_changeset(&changeset, path, &content))
}

fn record_from_changeset(
    changeset: &Changeset,
    path: Utf8PathBuf,
    content: &str,
) -> ChangesetRecord {
    ChangesetRecord {
        id: ChangesetId::new(&changeset.name),
        packages: changeset
            .packages
            .iter()
            .map(|package| ChangesetPackageInput {
                package: PackageId::new(&package.name),
                bump: core_bump(package.level),
                tag: package.tag.clone(),
            })
            .collect(),
        summary: changeset.summary.clone(),
        path,
        revision: FileHash::from_bytes(content.as_bytes()),
    }
}

fn changeset_from_record(record: &ChangesetRecord, root: &camino::Utf8Path) -> Changeset {
    let mut changeset = Changeset::new(record.id.to_string(), root.as_std_path());
    for package in &record.packages {
        if let Some(bump) = resolver_bump(package.bump) {
            changeset.add_package(package.package.to_string(), bump, package.tag.clone());
        }
    }
    changeset.summary(record.summary.clone());
    changeset
}

fn matching_existing(
    project: &Project,
    prepared: &PreparedChangeset,
) -> Result<ChangesetMutationResult, ChangesetCrudError> {
    let existing = read_record(project, prepared.record.path.clone())?;
    if existing.revision == prepared.record.revision {
        Ok(ChangesetMutationResult {
            status: ChangesetMutationStatus::Existing,
            changeset: existing,
        })
    } else {
        Err(ChangesetCrudError::Conflict {
            id: existing.id,
            actual: existing.revision,
        })
    }
}

fn verify_revision(
    current: &ChangesetRecord,
    expected: &FileHash,
) -> Result<(), ChangesetCrudError> {
    if &current.revision == expected {
        Ok(())
    } else {
        Err(ChangesetCrudError::RevisionMismatch {
            id: current.id.clone(),
            expected: expected.clone(),
            actual: current.revision.clone(),
        })
    }
}

fn sanitize_name(name: &str) -> String {
    const ILLEGAL_CHARS: [char; 8] = ['<', '>', ':', '"', '/', '\\', '|', ' '];
    name.chars()
        .map(|character| {
            if ILLEGAL_CHARS.contains(&character) {
                '-'
            } else {
                character.to_ascii_lowercase()
            }
        })
        .collect()
}

const fn resolver_bump(bump: BumpLevel) -> Option<semifold_resolver::changeset::BumpLevel> {
    match bump {
        BumpLevel::Unchanged => None,
        BumpLevel::Patch => Some(semifold_resolver::changeset::BumpLevel::Patch),
        BumpLevel::Minor => Some(semifold_resolver::changeset::BumpLevel::Minor),
        BumpLevel::Major => Some(semifold_resolver::changeset::BumpLevel::Major),
    }
}

const fn core_bump(bump: semifold_resolver::changeset::BumpLevel) -> BumpLevel {
    match bump {
        semifold_resolver::changeset::BumpLevel::Unchanged => BumpLevel::Unchanged,
        semifold_resolver::changeset::BumpLevel::Patch => BumpLevel::Patch,
        semifold_resolver::changeset::BumpLevel::Minor => BumpLevel::Minor,
        semifold_resolver::changeset::BumpLevel::Major => BumpLevel::Major,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use camino::Utf8PathBuf;
    use semifold_resolver::{
        config::{BranchesConfig, Config, PackageConfig, ReleaseChannel},
        resolver::ResolverType,
    };

    use super::*;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn project() -> Project {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock in tests must be after the Unix epoch")
            .as_nanos();
        let root = Utf8PathBuf::from_path_buf(std::env::temp_dir().join(format!(
            "semifold-changeset-service-{}-{nonce}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        )))
        .expect("temporary test directory must be valid UTF-8");
        let changeset_dir = root.join(".changes");
        fs::create_dir_all(&changeset_dir).expect("changeset fixture directory must be created");
        let config_path = changeset_dir.join("config.toml");
        Project {
            root,
            changeset_dir,
            config_path,
            config: Config {
                branches: BranchesConfig {
                    base: "main".to_string(),
                    release: "release".to_string(),
                },
                tags: BTreeMap::from([("feat".to_string(), "Features".to_string())]),
                changelog: Default::default(),
                packages: BTreeMap::from([(
                    "app".to_string(),
                    PackageConfig {
                        path: "app".into(),
                        resolver: ResolverType::Rust.into(),
                        publish: None,
                        channel: ReleaseChannel::Stable,
                        channel_bump: None,
                        assets: Vec::new(),
                        github_release: None,
                        depends_on: Vec::new(),
                    },
                )]),
                plugins: BTreeMap::new(),
                resolver: BTreeMap::new(),
            },
        }
    }

    fn draft(name: &str) -> ChangesetDraft {
        ChangesetDraft {
            name: name.to_string(),
            packages: vec![ChangesetPackageInput {
                package: PackageId::new("app"),
                bump: BumpLevel::Minor,
                tag: Some("feat".to_string()),
            }],
            summary: "Add application service".to_string(),
        }
    }

    #[test]
    fn creates_a_sanitized_changeset_that_the_resolver_can_read() {
        let project = project();

        let id = create_changeset(&project, draft("Application Service"))
            .expect("valid changeset draft must be created");
        let changesets = semifold_resolver::resolver::get_changesets(
            project.changeset_dir.as_std_path(),
            &project.config,
        )
        .expect("created changeset must be readable");

        assert_eq!(id.as_str(), "application-service");
        assert_eq!(changesets.len(), 1);
        assert_eq!(changesets[0].name, "application-service");
        fs::remove_dir_all(&project.root).expect("changeset fixture must be removed");
    }

    #[test]
    fn rejects_unknown_packages_before_writing() {
        let project = project();
        let mut input = draft("unknown-package");
        input.packages[0].package = PackageId::new("missing");

        let error = create_changeset(&project, input)
            .expect_err("unknown package changeset must be rejected");

        assert!(matches!(
            error,
            ChangesetCreateError::PackageNotFound { .. }
        ));
        assert!(!project.changeset_dir.join("unknown-package.md").exists());
        fs::remove_dir_all(&project.root).expect("changeset fixture must be removed");
    }

    #[test]
    fn create_is_idempotent_and_reports_different_existing_content() {
        let project = project();
        let dependencies = crate::SystemDependencies;

        let created = create_changeset_idempotent(
            &dependencies,
            &project,
            draft("application-service"),
            ExecutionMode::Apply,
        )
        .expect("new changeset must be created");
        let existing = create_changeset_idempotent(
            &dependencies,
            &project,
            draft("application-service"),
            ExecutionMode::Apply,
        )
        .expect("identical changeset creation must be idempotent");
        let mut different = draft("application-service");
        different.summary = "Different content".to_string();
        let conflict =
            create_changeset_idempotent(&dependencies, &project, different, ExecutionMode::Apply)
                .expect_err("different content must not replace an existing changeset");

        assert_eq!(created.status, ChangesetMutationStatus::Created);
        assert_eq!(existing.status, ChangesetMutationStatus::Existing);
        assert_eq!(created.changeset.revision, existing.changeset.revision);
        assert!(matches!(conflict, ChangesetCrudError::Conflict { .. }));
        fs::remove_dir_all(&project.root).expect("changeset fixture must be removed");
    }

    #[test]
    fn update_and_delete_require_the_latest_revision() {
        let project = project();
        let dependencies = crate::SystemDependencies;
        let created = create_changeset_idempotent(
            &dependencies,
            &project,
            draft("application-service"),
            ExecutionMode::Apply,
        )
        .expect("new changeset must be created");
        let mut replacement = draft("application-service");
        replacement.summary = "Updated application service".to_string();

        let updated = update_changeset(
            &dependencies,
            &project,
            "application-service",
            &created.changeset.revision,
            replacement,
            ExecutionMode::Apply,
        )
        .expect("matching revision must allow update");
        let stale_delete = delete_changeset(
            &dependencies,
            &project,
            "application-service",
            &created.changeset.revision,
            ExecutionMode::Apply,
        )
        .expect_err("stale revision must not allow deletion");
        let deleted = delete_changeset(
            &dependencies,
            &project,
            "application-service",
            &updated.changeset.revision,
            ExecutionMode::Apply,
        )
        .expect("latest revision must allow deletion");

        assert_eq!(updated.status, ChangesetMutationStatus::Updated);
        assert!(matches!(
            stale_delete,
            ChangesetCrudError::RevisionMismatch { .. }
        ));
        assert_eq!(deleted.status, ChangesetMutationStatus::Deleted);
        assert!(
            !project
                .changeset_dir
                .join("application-service.md")
                .exists()
        );
        fs::remove_dir_all(&project.root).expect("changeset fixture must be removed");
    }

    #[test]
    fn dry_run_validates_mutations_without_changing_files() {
        let project = project();
        let dependencies = crate::SystemDependencies;

        let planned_create = create_changeset_idempotent(
            &dependencies,
            &project,
            draft("planned"),
            ExecutionMode::DryRun,
        )
        .expect("valid dry-run creation must be planned");
        assert_eq!(planned_create.status, ChangesetMutationStatus::Planned);
        assert!(!project.changeset_dir.join("planned.md").exists());

        let created = create_changeset_idempotent(
            &dependencies,
            &project,
            draft("application-service"),
            ExecutionMode::Apply,
        )
        .expect("fixture changeset must be created");
        let original_content = fs::read_to_string(&created.changeset.path)
            .expect("fixture changeset must remain readable");
        let mut replacement = draft("application-service");
        replacement.summary = "Planned replacement".to_string();
        let planned_update = update_changeset(
            &dependencies,
            &project,
            "application-service",
            &created.changeset.revision,
            replacement,
            ExecutionMode::DryRun,
        )
        .expect("valid dry-run update must be planned");
        let planned_delete = delete_changeset(
            &dependencies,
            &project,
            "application-service",
            &created.changeset.revision,
            ExecutionMode::DryRun,
        )
        .expect("valid dry-run delete must be planned");

        assert_eq!(planned_update.status, ChangesetMutationStatus::Planned);
        assert_eq!(planned_delete.status, ChangesetMutationStatus::Planned);
        assert_eq!(
            fs::read_to_string(&created.changeset.path)
                .expect("dry-run changeset must remain readable"),
            original_content
        );
        fs::remove_dir_all(&project.root).expect("changeset fixture must be removed");
    }

    #[test]
    fn malformed_changesets_return_parse_errors_without_panicking() {
        let project = project();
        fs::write(
            project.changeset_dir.join("broken.md"),
            "---\napp: invalid\n---\n\nBroken.\n",
        )
        .expect("malformed changeset fixture must be written");

        let error = get_changesets(&project, Some("broken"))
            .expect_err("malformed changeset must be reported as an error");

        assert!(matches!(error, ChangesetCrudError::Parse { .. }));
        fs::remove_dir_all(&project.root).expect("changeset fixture must be removed");
    }
}
