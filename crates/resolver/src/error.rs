use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Invalid changeset {path}: {reason}")]
    InvalidChangeset {
        path: PathBuf,
        reason: String,
    },
}