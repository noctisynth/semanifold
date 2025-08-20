use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Invalid changeset {path}: {reason}")]
    InvalidChangeset { path: PathBuf, reason: String },
    #[error("Invalid config {path}: {reason}")]
    InvalidConfig { path: PathBuf, reason: String },
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("File or directory not found: {path}")]
    FileOrDirNotFound { path: PathBuf },
    #[error("Invalid version {version}: {reason}")]
    InvalidVersion { version: String, reason: String },
}
