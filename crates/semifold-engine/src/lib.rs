#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod project;

pub use project::{Project, ProjectLoadError, ProjectLocation};
