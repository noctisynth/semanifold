use rust_i18n::t;
use semifold_changelog::github::{GitHubFailure, GitHubOperation, sanitize};

pub(crate) fn render(error: &GitHubFailure) -> String {
    let operation = match error.operation {
        GitHubOperation::Initialize => t!("cli.github.operations.initialize"),
        GitHubOperation::ListPullRequests => t!("cli.github.operations.list_pull_requests"),
        GitHubOperation::CreatePullRequest => t!("cli.github.operations.create_pull_request"),
        GitHubOperation::UpdatePullRequest => t!("cli.github.operations.update_pull_request"),
        GitHubOperation::ListComments => t!("cli.github.operations.list_comments"),
        GitHubOperation::ListFiles => t!("cli.github.operations.list_files"),
        GitHubOperation::CreateComment => t!("cli.github.operations.create_comment"),
        GitHubOperation::UpdateComment => t!("cli.github.operations.update_comment"),
        GitHubOperation::CreateRelease => t!("cli.github.operations.create_release"),
        GitHubOperation::UploadAsset => t!("cli.github.operations.upload_asset"),
        GitHubOperation::QueryCommitPullRequest => {
            t!("cli.github.operations.query_commit_pull_request")
        }
    };
    let diagnostic = &error.diagnostic;
    let Some(status) = &diagnostic.status else {
        return t!(
            "cli.github.client_failed",
            operation = operation,
            error = sanitize(&diagnostic.message)
        )
        .into_owned();
    };
    let mut lines = vec![
        t!("cli.github.failed", operation = operation).into_owned(),
        t!(
            "cli.github.api_error",
            status = status,
            message = sanitize(&diagnostic.message)
        )
        .into_owned(),
    ];
    for detail in &diagnostic.details {
        lines.push(t!("cli.github.detail", detail = sanitize(detail)).into_owned());
    }
    if diagnostic.status_code == Some(403) {
        lines.push(
            match error.operation {
                GitHubOperation::CreateRelease | GitHubOperation::UploadAsset => {
                    t!("cli.github.release_permission_hint")
                }
                GitHubOperation::CreateComment
                | GitHubOperation::UpdateComment
                | GitHubOperation::ListComments => t!("cli.github.comment_permission_hint"),
                _ => t!("cli.github.pr_permission_hint"),
            }
            .into_owned(),
        );
    }
    if let Some(url) = &diagnostic.documentation_url {
        lines.push(t!("cli.github.documentation", url = sanitize(url)).into_owned());
    }
    lines.join("\n    ")
}

pub(crate) fn render_command_error(error: &anyhow::Error) -> String {
    if let Some(failure) = error
        .chain()
        .find_map(|source| source.downcast_ref::<GitHubFailure>())
    {
        return render(failure);
    }
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use semifold_changelog::github::GitHubDiagnostic;

    fn failure(operation: GitHubOperation, code: u16) -> GitHubFailure {
        GitHubFailure {
            operation,
            diagnostic: GitHubDiagnostic {
                status: Some(code.to_string()),
                status_code: Some(code),
                message: "denied".into(),
                details: vec!["tag_name: invalid".into()],
                documentation_url: Some("https://docs.github.com/rest".into()),
            },
        }
    }

    #[test]
    fn api_errors_preserve_details_and_only_403_adds_permission_guidance() {
        for operation in [
            GitHubOperation::CreatePullRequest,
            GitHubOperation::UpdatePullRequest,
            GitHubOperation::CreateRelease,
            GitHubOperation::UploadAsset,
            GitHubOperation::CreateComment,
            GitHubOperation::UpdateComment,
            GitHubOperation::ListComments,
            GitHubOperation::ListFiles,
            GitHubOperation::ListPullRequests,
            GitHubOperation::QueryCommitPullRequest,
        ] {
            for code in [403, 422, 500] {
                let rendered = render(&failure(operation, code));
                assert!(rendered.contains(&code.to_string()));
                assert!(rendered.contains("denied"));
                assert!(rendered.contains("tag_name: invalid"));
                assert!(rendered.contains("https://docs.github.com/rest"));
                assert_eq!(rendered.lines().count(), if code == 403 { 5 } else { 4 });
            }
        }
    }

    #[test]
    fn command_boundary_preserves_nested_github_failure() {
        let failure = failure(GitHubOperation::CreateRelease, 403);
        let expected = render(&failure);
        let error = anyhow::Error::new(failure).context("publish setup");
        assert_eq!(render_command_error(&error), expected);
    }

    #[test]
    fn client_error_stays_on_one_line() {
        let error = GitHubFailure {
            operation: GitHubOperation::Initialize,
            diagnostic: GitHubDiagnostic {
                status: None,
                status_code: None,
                message: "request: connection closed".into(),
                details: vec![],
                documentation_url: None,
            },
        };
        let rendered = render(&error);
        assert!(rendered.contains("request: connection closed"));
        assert_eq!(rendered.lines().count(), 1);
    }
}
