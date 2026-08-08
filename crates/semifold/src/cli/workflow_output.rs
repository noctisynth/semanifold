use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::Serialize;

static DELIMITER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) const VERSION_OUTPUT_KEY: &str = "semifold-version";
pub(crate) const PUBLISH_OUTPUT_KEY: &str = "semifold-publish";

pub(crate) struct GithubOutputWriter {
    path: Option<PathBuf>,
}

impl GithubOutputWriter {
    pub(crate) fn from_environment() -> Self {
        let path = (std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"))
            .then(|| std::env::var_os("GITHUB_OUTPUT").map(PathBuf::from))
            .flatten();
        Self { path }
    }

    pub(crate) fn write<T: Serialize>(&self, key: &str, value: &T) -> anyhow::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let payload = serde_json::to_string(value)?;
        write_output(path, key, &payload)?;
        Ok(())
    }
}

fn write_output(path: &Path, key: &str, payload: &str) -> std::io::Result<()> {
    let sequence = DELIMITER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut delimiter = format!("semifold_{}_{}", std::process::id(), sequence);
    while payload.contains(&delimiter) {
        let sequence = DELIMITER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        delimiter = format!("semifold_{}_{}", std::process::id(), sequence);
    }
    let record = format!("{key}<<{delimiter}\n{payload}\n{delimiter}\n");
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(record.as_bytes())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde::Serialize;

    use super::*;

    #[derive(Serialize)]
    struct Output<'a> {
        value: &'a str,
    }

    #[test]
    fn writes_github_multiline_output_without_interpreting_payload() {
        let path = std::env::temp_dir().join(format!(
            "semifold-workflow-output-{}-{}",
            std::process::id(),
            DELIMITER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        write_output(
            &path,
            "result",
            &serde_json::to_string(&Output { value: "a\nb" }).unwrap(),
        )
        .unwrap();
        let content = fs::read_to_string(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert!(content.starts_with("result<<semifold_"));
        assert!(content.contains("{\"value\":\"a\\nb\"}"));
        assert_eq!(content.lines().count(), 3);
    }
}
