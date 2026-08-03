use std::collections::BTreeSet;

use semifold_core::{BumpLevel, ChangesetId, PackageId};
use semifold_resolver::{changeset::Changeset, error::ResolveError};
use thiserror::Error;

use crate::Project;

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
    if project.changeset_dir.join(format!("{name}.md")).exists() {
        return Err(ChangesetCreateError::AlreadyExists { name });
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
    changeset.commit().map_err(ChangesetCreateError::Write)?;
    Ok(ChangesetId::new(name))
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
                        resolver: ResolverType::Rust,
                        channel: ReleaseChannel::Stable,
                        channel_bump: None,
                        assets: Vec::new(),
                        github_release: None,
                        depends_on: Vec::new(),
                    },
                )]),
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
}
