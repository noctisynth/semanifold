//! GitHub diagnostic facts shared by changelog, release automation, and CLI presentation.
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHubOperation {
    Initialize,
    ListPullRequests,
    CreatePullRequest,
    UpdatePullRequest,
    ListComments,
    ListFiles,
    CreateComment,
    UpdateComment,
    CreateRelease,
    UploadAsset,
    QueryCommitPullRequest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitHubDiagnostic {
    pub status: Option<String>,
    pub status_code: Option<u16>,
    pub message: String,
    pub details: Vec<String>,
    pub documentation_url: Option<String>,
}

impl From<octocrab::Error> for GitHubDiagnostic {
    fn from(error: octocrab::Error) -> Self {
        if let octocrab::Error::GitHub { source, .. } = error {
            Self {
                status: Some(source.status_code.to_string()),
                status_code: Some(source.status_code.as_u16()),
                message: sanitize(&source.message),
                details: source
                    .errors
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|value| sanitize(&value.to_string()))
                    .collect(),
                documentation_url: source.documentation_url.as_deref().map(sanitize),
            }
        } else {
            Self {
                status: None,
                status_code: None,
                message: format_error_chain(&error),
                details: Vec::new(),
                documentation_url: None,
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitHubFailure {
    pub operation: GitHubOperation,
    pub diagnostic: GitHubDiagnostic,
}

impl GitHubFailure {
    pub fn new(operation: GitHubOperation, error: octocrab::Error) -> Self {
        Self {
            operation,
            diagnostic: error.into(),
        }
    }
}

impl fmt::Display for GitHubFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(status) = &self.diagnostic.status {
            write!(f, "{status}: ")?;
        }
        f.write_str(&self.diagnostic.message)?;
        for detail in &self.diagnostic.details {
            write!(f, "; {detail}")?;
        }
        if let Some(url) = &self.diagnostic.documentation_url {
            write!(f, "; {url}")?;
        }
        Ok(())
    }
}

impl std::error::Error for GitHubFailure {}

pub fn format_error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut messages = vec![sanitize(&error.to_string())];
    let mut source = error.source();
    while let Some(error) = source {
        let message = sanitize(&error.to_string());
        if messages.last() != Some(&message) {
            messages.push(message);
        }
        source = error.source();
    }
    messages.join(": ")
}

pub fn sanitize(value: &str) -> String {
    let tokens = ["GITHUB_TOKEN", "GH_TOKEN"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok());
    sanitize_with_tokens(value, tokens)
}

fn sanitize_with_tokens(value: &str, tokens: impl IntoIterator<Item = String>) -> String {
    let mut value = value.to_owned();
    for token in tokens {
        if !token.is_empty() {
            value = value.replace(&token, "[REDACTED]");
        }
    }
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials_before_normalizing_control_characters() {
        assert_eq!(
            sanitize_with_tokens("token secret\nvalue\rdenied", ["secret\nvalue".into()]),
            "token [REDACTED] denied"
        );
    }

    #[test]
    fn preserves_nested_client_causes_and_removes_control_characters() {
        #[derive(Debug)]
        struct Outer(std::io::Error);
        impl fmt::Display for Outer {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("request")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        assert_eq!(
            format_error_chain(&Outer(std::io::Error::other("connection\nclosed"))),
            "request: connection closed"
        );
    }
}
