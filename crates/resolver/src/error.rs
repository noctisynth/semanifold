use std::{path::PathBuf, process::ExitStatus};

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Invalid changeset {path}: {reason}")]
    InvalidChangeset { path: PathBuf, reason: String },
    #[error("Invalid config {path}: {reason}")]
    InvalidConfig { path: PathBuf, reason: String },
    #[error("Invalid changelog {path}: {reason}")]
    InvalidChangelog { path: PathBuf, reason: String },
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("File or directory not found: {path}")]
    FileOrDirNotFound { path: PathBuf },
    #[error("Invalid version {version}: {reason}")]
    InvalidVersion { version: String, reason: String },
    #[error("Failed to parse {path}: {reason}")]
    ParseError { path: PathBuf, reason: String },
    #[error("Command {command} failed: {status} (code {code:?})")]
    CommandError {
        command: String,
        status: ExitStatus,
        code: Option<i32>,
    },
    #[error("Git error: {message}")]
    GitError { message: String },
    #[error("GitHub error: {message}")]
    GitHubError { message: String },
    #[error("Pre-release tag {tag} is invalid: {message}")]
    PreReleaseTagInvalid { tag: String, message: String },
    #[error("Semver error: {0}")]
    SemverError(#[from] semver::Error),
}
