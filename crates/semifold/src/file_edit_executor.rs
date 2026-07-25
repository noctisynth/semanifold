use std::{
    collections::BTreeSet,
    error::Error,
    fmt, fs,
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use rust_i18n::t;
use semifold_core::{FileEdit, FileEditExpectation, FileHash};

static NEXT_TEMPORARY_FILE: AtomicU64 = AtomicU64::new(0);

/// Applies pre-planned file replacements only after all targets pass validation.
pub struct FileEditExecutor<'root> {
    project_root: &'root Utf8Path,
}

impl<'root> FileEditExecutor<'root> {
    #[must_use]
    pub const fn new(project_root: &'root Utf8Path) -> Self {
        Self { project_root }
    }

    pub fn apply(&self, edits: &[FileEdit]) -> Result<FileEditApplyReport, FileEditApplyError> {
        let targets = validate_targets(self.project_root, edits)?;
        let mut temporary_files = Vec::with_capacity(edits.len());

        for (target, edit) in targets.iter().zip(edits) {
            let temporary = temporary_path(&target.path);
            let result = (|| -> Result<(), FileEditApplyError> {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temporary)
                    .map_err(|source| FileEditApplyError::CreateTemporary {
                        path: temporary.clone(),
                        source,
                    })?;
                file.write_all(edit.new_content.as_bytes())
                    .and_then(|()| file.sync_all())
                    .map_err(|source| FileEditApplyError::WriteTemporary {
                        path: temporary.clone(),
                        source,
                    })
            })();
            if let Err(error) = result {
                cleanup_temporary_files(&temporary_files);
                let _ = fs::remove_file(&temporary);
                return Err(error);
            }
            temporary_files.push(temporary);
        }

        let mut applied = Vec::with_capacity(edits.len());
        for (target, temporary) in targets.iter().zip(&temporary_files) {
            let result = match &target.expectation {
                FileEditExpectation::Existing { .. } => fs::rename(temporary, &target.path),
                FileEditExpectation::Missing => {
                    fs::hard_link(temporary, &target.path).and_then(|()| fs::remove_file(temporary))
                }
            };
            if let Err(source) = result {
                cleanup_temporary_files(&temporary_files[applied.len() + 1..]);
                return Err(FileEditApplyError::Replace {
                    path: target.path.clone(),
                    applied,
                    source,
                });
            }
            applied.push(target.path.clone());
        }
        Ok(FileEditApplyReport { applied })
    }
}

/// Validates every planned target without creating or replacing files.
pub fn validate_file_edits(
    project_root: &Utf8Path,
    edits: &[FileEdit],
) -> Result<(), FileEditApplyError> {
    validate_targets(project_root, edits).map(|_| ())
}

fn validate_targets(
    project_root: &Utf8Path,
    edits: &[FileEdit],
) -> Result<Vec<ValidatedTarget>, FileEditApplyError> {
    let mut paths = BTreeSet::new();
    let mut targets = Vec::with_capacity(edits.len());
    for edit in edits {
        let path = &edit.path;
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Utf8Component::ParentDir))
        {
            return Err(FileEditApplyError::InvalidPath { path: path.clone() });
        }
        if !paths.insert(path.clone()) {
            return Err(FileEditApplyError::DuplicateTarget { path: path.clone() });
        }
        let target = project_root.join(path);
        match &edit.expected {
            FileEditExpectation::Existing { hash } => {
                let content = fs::read(&target).map_err(|source| FileEditApplyError::Read {
                    path: target.clone(),
                    source,
                })?;
                let actual = FileHash::from_bytes(&content);
                if actual != *hash {
                    return Err(FileEditApplyError::HashMismatch {
                        path: target,
                        expected: hash.clone(),
                        actual,
                    });
                }
            }
            FileEditExpectation::Missing => match fs::symlink_metadata(&target) {
                Ok(_) => return Err(FileEditApplyError::TargetExists { path: target }),
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(FileEditApplyError::Read {
                        path: target,
                        source,
                    });
                }
            },
        }
        targets.push(ValidatedTarget {
            path: target,
            expectation: edit.expected.clone(),
        });
    }
    Ok(targets)
}

/// Files successfully replaced by one completed apply operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEditApplyReport {
    pub applied: Vec<Utf8PathBuf>,
}

#[derive(Clone)]
struct ValidatedTarget {
    path: Utf8PathBuf,
    expectation: FileEditExpectation,
}

fn temporary_path(target: &Utf8Path) -> Utf8PathBuf {
    let suffix = NEXT_TEMPORARY_FILE.fetch_add(1, Ordering::Relaxed);
    let file_name = target.file_name().unwrap_or("edit");
    target.parent().unwrap_or(target).join(format!(
        ".{file_name}.smif-{}-{suffix}.tmp",
        std::process::id()
    ))
}

fn cleanup_temporary_files(files: &[Utf8PathBuf]) {
    for file in files {
        let _ = fs::remove_file(file);
    }
}

#[derive(Debug)]
pub enum FileEditApplyError {
    InvalidPath {
        path: Utf8PathBuf,
    },
    DuplicateTarget {
        path: Utf8PathBuf,
    },
    Read {
        path: Utf8PathBuf,
        source: std::io::Error,
    },
    HashMismatch {
        path: Utf8PathBuf,
        expected: FileHash,
        actual: FileHash,
    },
    TargetExists {
        path: Utf8PathBuf,
    },
    CreateTemporary {
        path: Utf8PathBuf,
        source: std::io::Error,
    },
    WriteTemporary {
        path: Utf8PathBuf,
        source: std::io::Error,
    },
    Replace {
        path: Utf8PathBuf,
        applied: Vec<Utf8PathBuf>,
        source: std::io::Error,
    },
}

impl fmt::Display for FileEditApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath { path } => write!(
                formatter,
                "{}",
                t!("cli.version.edit_path_invalid", path = path)
            ),
            Self::DuplicateTarget { path } => write!(
                formatter,
                "{}",
                t!("cli.version.duplicate_edit", path = path)
            ),
            Self::Read { path, source } => write!(
                formatter,
                "{}",
                t!("cli.version.edit_read_failed", path = path, error = source)
            ),
            Self::HashMismatch { path, .. } => write!(
                formatter,
                "{}",
                t!("cli.version.edit_hash_mismatch", path = path)
            ),
            Self::TargetExists { path } => write!(
                formatter,
                "{}",
                t!("cli.version.edit_target_exists", path = path)
            ),
            Self::CreateTemporary { path, source } => write!(
                formatter,
                "{}",
                t!(
                    "cli.version.edit_temp_create_failed",
                    path = path,
                    error = source
                )
            ),
            Self::WriteTemporary { path, source } => write!(
                formatter,
                "{}",
                t!(
                    "cli.version.edit_temp_write_failed",
                    path = path,
                    error = source
                )
            ),
            Self::Replace { path, source, .. } => write!(
                formatter,
                "{}",
                t!(
                    "cli.version.edit_replace_failed",
                    path = path,
                    error = source
                )
            ),
        }
    }
}

impl Error for FileEditApplyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. }
            | Self::CreateTemporary { source, .. }
            | Self::WriteTemporary { source, .. }
            | Self::Replace { source, .. } => Some(source),
            Self::InvalidPath { .. }
            | Self::DuplicateTarget { .. }
            | Self::HashMismatch { .. }
            | Self::TargetExists { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use semifold_core::{EditSource, FileEdit, FileEditExpectation, FileHash, PackageId};

    use super::{FileEditApplyError, FileEditExecutor};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn temporary_root() -> camino::Utf8PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "semifold-file-edit-{}-{nonce}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        camino::Utf8PathBuf::from_path_buf(root).unwrap()
    }

    fn edit(path: &str, old: &str, new: &str) -> FileEdit {
        FileEdit {
            path: path.into(),
            expected: FileEditExpectation::Existing {
                hash: FileHash::from_bytes(old.as_bytes()),
            },
            new_content: new.to_string(),
            source: EditSource::PackageVersion {
                package: PackageId::new("app"),
            },
        }
    }

    #[test]
    fn replaces_all_validated_files() {
        let root = temporary_root();
        fs::write(root.join("one.txt"), "one").unwrap();
        fs::write(root.join("two.txt"), "two").unwrap();

        let report = FileEditExecutor::new(&root)
            .apply(&[
                edit("one.txt", "one", "next-one"),
                edit("two.txt", "two", "next-two"),
            ])
            .unwrap();

        assert_eq!(report.applied, [root.join("one.txt"), root.join("two.txt")]);

        assert_eq!(
            fs::read_to_string(root.join("one.txt")).unwrap(),
            "next-one"
        );
        assert_eq!(
            fs::read_to_string(root.join("two.txt")).unwrap(),
            "next-two"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_a_changed_target_before_writing_any_file() {
        let root = temporary_root();
        fs::write(root.join("one.txt"), "one").unwrap();
        fs::write(root.join("two.txt"), "changed").unwrap();

        let error = FileEditExecutor::new(&root)
            .apply(&[
                edit("one.txt", "one", "next-one"),
                edit("two.txt", "two", "next-two"),
            ])
            .unwrap_err();

        assert!(matches!(error, FileEditApplyError::HashMismatch { .. }));
        assert_eq!(fs::read_to_string(root.join("one.txt")).unwrap(), "one");
        assert_eq!(fs::read_to_string(root.join("two.txt")).unwrap(), "changed");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cleans_prepared_temporary_files_when_a_later_prepare_fails() {
        let root = temporary_root();
        fs::write(root.join("one.txt"), "one").unwrap();
        let missing_parent_edit = FileEdit {
            path: "missing/two.txt".into(),
            expected: FileEditExpectation::Missing,
            new_content: "next-two".to_string(),
            source: EditSource::PackageVersion {
                package: PackageId::new("two"),
            },
        };

        let error = FileEditExecutor::new(&root)
            .apply(&[edit("one.txt", "one", "next-one"), missing_parent_edit])
            .unwrap_err();

        assert!(matches!(error, FileEditApplyError::CreateTemporary { .. }));
        assert_eq!(fs::read_to_string(root.join("one.txt")).unwrap(), "one");
        assert!(!root.join("missing/two.txt").exists());
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".smif-")
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_duplicate_or_escaping_targets_before_writing() {
        let root = temporary_root();
        fs::write(root.join("one.txt"), "one").unwrap();
        let executor = FileEditExecutor::new(&root);

        let duplicate = executor
            .apply(&[
                edit("one.txt", "one", "first"),
                edit("one.txt", "one", "second"),
            ])
            .unwrap_err();
        assert!(matches!(
            duplicate,
            FileEditApplyError::DuplicateTarget { .. }
        ));

        let escaping = executor
            .apply(&[edit("../outside.txt", "", "bad")])
            .unwrap_err();
        assert!(matches!(escaping, FileEditApplyError::InvalidPath { .. }));
        assert_eq!(fs::read_to_string(root.join("one.txt")).unwrap(), "one");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn creates_a_missing_target_without_overwriting_existing_files() {
        let root = temporary_root();
        let executor = FileEditExecutor::new(&root);
        let create = FileEdit {
            path: "CHANGELOG.md".into(),
            expected: FileEditExpectation::Missing,
            new_content: "# Changelog\n".to_string(),
            source: EditSource::Changelog {
                package: PackageId::new("app"),
            },
        };

        let report = executor.apply(std::slice::from_ref(&create)).unwrap();
        assert_eq!(report.applied, [root.join("CHANGELOG.md")]);
        assert_eq!(
            fs::read_to_string(root.join("CHANGELOG.md")).unwrap(),
            "# Changelog\n"
        );
        assert!(matches!(
            executor.apply(&[create]).unwrap_err(),
            FileEditApplyError::TargetExists { .. }
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
