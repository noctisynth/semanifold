use std::{
    error::Error,
    fmt, io,
    path::{Component, Path, PathBuf},
};

use camino::Utf8PathBuf;

/// Converts a package path into a stable project-relative UTF-8 path.
pub(crate) fn normalize_package_path(
    project_root: &Path,
    package_path: &Path,
) -> Result<Utf8PathBuf, PackagePathError> {
    if !project_root.is_absolute() || !project_root.is_dir() {
        return Err(PackagePathError::InvalidProjectRoot {
            root: project_root.to_path_buf(),
        });
    }

    let project_root =
        lexical_normalize(project_root).ok_or_else(|| PackagePathError::InvalidProjectRoot {
            root: project_root.to_path_buf(),
        })?;
    let candidate = if package_path.is_absolute() {
        package_path.to_path_buf()
    } else {
        project_root.join(package_path)
    };
    let candidate =
        lexical_normalize(&candidate).ok_or_else(|| PackagePathError::EscapesProjectRoot {
            root: project_root.clone(),
            path: package_path.to_path_buf(),
        })?;
    let relative = candidate.strip_prefix(&project_root).map_err(|_| {
        PackagePathError::EscapesProjectRoot {
            root: project_root.clone(),
            path: package_path.to_path_buf(),
        }
    })?;

    let canonical_root =
        std::fs::canonicalize(&project_root).map_err(|source| PackagePathError::InspectFailed {
            path: project_root.clone(),
            source,
        })?;
    let canonical_ancestor = canonical_existing_ancestor(&candidate)?;
    if !canonical_ancestor.starts_with(&canonical_root) {
        return Err(PackagePathError::SymlinkEscapesProjectRoot {
            root: project_root,
            path: package_path.to_path_buf(),
        });
    }

    relative_to_utf8(relative)
}

fn lexical_normalize(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
        }
    }
    Some(normalized)
}

fn canonical_existing_ancestor(path: &Path) -> Result<PathBuf, PackagePathError> {
    let mut current = path;
    loop {
        match std::fs::symlink_metadata(current) {
            Ok(_) => {
                return std::fs::canonicalize(current).map_err(|source| {
                    PackagePathError::InspectFailed {
                        path: current.to_path_buf(),
                        source,
                    }
                });
            }
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                current = current
                    .parent()
                    .ok_or_else(|| PackagePathError::InspectFailed {
                        path: path.to_path_buf(),
                        source,
                    })?;
            }
            Err(source) => {
                return Err(PackagePathError::InspectFailed {
                    path: current.to_path_buf(),
                    source,
                });
            }
        }
    }
}

fn relative_to_utf8(path: &Path) -> Result<Utf8PathBuf, PackagePathError> {
    let mut segments = Vec::new();
    for component in path.components() {
        let Component::Normal(segment) = component else {
            continue;
        };
        segments.push(segment.to_str().ok_or_else(|| PackagePathError::NonUtf8 {
            path: path.to_path_buf(),
        })?);
    }
    if segments.is_empty() {
        Ok(Utf8PathBuf::from("."))
    } else {
        Ok(Utf8PathBuf::from(segments.join("/")))
    }
}

#[derive(Debug)]
pub(crate) enum PackagePathError {
    InvalidProjectRoot { root: PathBuf },
    EscapesProjectRoot { root: PathBuf, path: PathBuf },
    SymlinkEscapesProjectRoot { root: PathBuf, path: PathBuf },
    NonUtf8 { path: PathBuf },
    InspectFailed { path: PathBuf, source: io::Error },
}

impl fmt::Display for PackagePathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProjectRoot { root } => {
                write!(
                    formatter,
                    "project root is not an existing absolute directory: {}",
                    root.display()
                )
            }
            Self::EscapesProjectRoot { root, path } => write!(
                formatter,
                "package path {} escapes project root {}",
                path.display(),
                root.display()
            ),
            Self::SymlinkEscapesProjectRoot { root, path } => write!(
                formatter,
                "package path {} resolves through a symlink outside project root {}",
                path.display(),
                root.display()
            ),
            Self::NonUtf8 { path } => {
                write!(
                    formatter,
                    "package path is not valid UTF-8: {}",
                    path.display()
                )
            }
            Self::InspectFailed { path, source } => {
                write!(
                    formatter,
                    "failed to inspect package path {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl Error for PackagePathError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InspectFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    struct TemporaryRoot(PathBuf);

    impl TemporaryRoot {
        fn new(name: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "semifold-package-path-{name}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TemporaryRoot {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn normalizes_relative_absolute_root_and_missing_paths() {
        let root = TemporaryRoot::new("normal");
        fs::create_dir_all(root.0.join("crates/app")).unwrap();

        assert_eq!(
            normalize_package_path(&root.0, Path::new("./crates/temp/../app")).unwrap(),
            "crates/app"
        );
        assert_eq!(
            normalize_package_path(&root.0, &root.0.join("crates/app")).unwrap(),
            "crates/app"
        );
        assert_eq!(
            normalize_package_path(&root.0, Path::new(".")).unwrap(),
            "."
        );
        assert_eq!(
            normalize_package_path(&root.0, Path::new("crates/future")).unwrap(),
            "crates/future"
        );
    }

    #[test]
    fn rejects_relative_and_absolute_paths_outside_the_project() {
        let root = TemporaryRoot::new("escape");
        let outside = root.0.parent().unwrap().join("outside");

        assert!(matches!(
            normalize_package_path(&root.0, Path::new("../outside")),
            Err(PackagePathError::EscapesProjectRoot { .. })
        ));
        assert!(matches!(
            normalize_package_path(&root.0, &outside),
            Err(PackagePathError::EscapesProjectRoot { .. })
        ));
    }

    #[test]
    fn requires_an_existing_absolute_project_root() {
        assert!(matches!(
            normalize_package_path(Path::new("relative"), Path::new("package")),
            Err(PackagePathError::InvalidProjectRoot { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_package_paths() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let root = TemporaryRoot::new("non-utf8");
        let path = PathBuf::from(OsString::from_vec(vec![b'p', b'k', b'g', 0xff]));

        assert!(matches!(
            normalize_package_path(&root.0, &path),
            Err(PackagePathError::NonUtf8 { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn accepts_internal_symlinks_and_rejects_external_symlinks() {
        use std::os::unix::fs::symlink;

        let root = TemporaryRoot::new("symlink-root");
        let outside = TemporaryRoot::new("symlink-outside");
        fs::create_dir_all(root.0.join("real/package")).unwrap();
        symlink(root.0.join("real"), root.0.join("internal-link")).unwrap();
        symlink(&outside.0, root.0.join("external-link")).unwrap();

        assert_eq!(
            normalize_package_path(&root.0, Path::new("internal-link/package")).unwrap(),
            "internal-link/package"
        );
        assert!(matches!(
            normalize_package_path(&root.0, Path::new("external-link/missing")),
            Err(PackagePathError::SymlinkEscapesProjectRoot { .. })
        ));
    }
}
