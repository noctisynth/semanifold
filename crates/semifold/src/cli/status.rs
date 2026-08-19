use std::{collections::BTreeSet, env};

use anyhow::Context as _;
use camino::Utf8Path;
use clap::Parser;
use colored::Colorize;
use octocrab::{
    Octocrab,
    models::issues::Comment,
    models::repos::{DiffEntry, DiffEntryStatus},
};
use rust_i18n::t;
use semifold_core::{PackageRelease, PlanWarning, ReleasePlan, ReleaseReason};
use semifold_engine::{Project, SemifoldService, SystemDependencies};
use serde::{Deserialize, Serialize};

use crate::cli::terminal::{StepOutcome, Terminal};

const COMMENT_MARKER: &str = "<!-- semifold:release-plan -->";
const LEGACY_COMMENT_PREFIX: &str = "## Workspace change through:";

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct RepoOwner {
    pub login: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Repository {
    pub name: String,
    pub owner: RepoOwner,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Branch {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
    pub label: String,
    pub repo: Repository,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct PullRequest {
    pub number: u64,
    pub head: Branch,
    pub base: Branch,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct GitHubEvent {
    pub repository: Repository,
    pub pull_request: PullRequest,
}

#[derive(Parser, Debug)]
pub(crate) struct Status {
    #[arg(short, long, default_value_t = true, help = t!("cli.status.flags.comment"))]
    pub comment: bool,
}

pub(crate) async fn run(status: &Status, project: &Project) -> anyhow::Result<()> {
    let terminal = Terminal::detect();
    terminal.heading(&t!("cli.status.heading"));
    let is_ci = env::var("GITHUB_ACTIONS").is_ok();
    let config = &project.config;
    log::debug!("GitHub CI environment: {}", is_ci);

    let progress = terminal.progress(t!("cli.status.planning").into_owned());
    let plan = match SemifoldService::new(SystemDependencies).plan_release(project) {
        Ok(plan) => plan,
        Err(error) => {
            progress.finish(
                StepOutcome::Failed,
                t!("cli.status.planning_failed").into_owned(),
            );
            return Err(anyhow::anyhow!(t!("cli.status.plan_failed", error = error)));
        }
    };
    progress.finish(
        StepOutcome::Success,
        t!("cli.status.planned", count = plan.packages().len()).into_owned(),
    );
    let name_width = plan
        .packages()
        .iter()
        .map(|package| package.id.as_str().len())
        .max()
        .unwrap_or(0)
        + 1;

    terminal.fact(
        &t!("cli.status.fact_changesets"),
        plan.consumed_changesets().len(),
    );
    terminal.fact(&t!("cli.status.fact_packages"), plan.packages().len());
    terminal.fact(
        &t!("cli.status.fact_fingerprint"),
        semifold_core::ReleaseContext::from_plan(&plan)
            .plan
            .fingerprint,
    );
    terminal.blank();

    let warnings = plan
        .warnings()
        .iter()
        .map(|warning| match warning {
            PlanWarning::NonPatchBumpOnPrerelease { package, .. } => {
                t!(
                    "cli.status.pre_release_warning",
                    package = package.as_str().cyan()
                )
            }
        })
        .collect::<Vec<_>>();

    if plan.packages().is_empty() {
        terminal.summary(StepOutcome::Skipped, &t!("cli.status.no_packages"));
    } else {
        terminal.section(&t!("cli.status.packages"));
        terminal.line(format!(
            "  {} {} {} {} {}",
            Terminal::cell(t!("cli.status.column_package"), name_width),
            Terminal::cell(t!("cli.status.column_current"), 16),
            Terminal::cell(t!("cli.status.column_next"), 16),
            Terminal::cell(t!("cli.status.column_bump"), 8),
            t!("cli.status.column_reason")
        ));
        for package in plan.packages() {
            let reason = render_reasons(package);
            terminal.line(format!(
                "  {} {} {} {} {}",
                Terminal::cell(package.id.as_str(), name_width)
                    .cyan()
                    .bold(),
                Terminal::cell(package.current_version.to_string(), 16).yellow(),
                Terminal::cell(package.next_version.to_string(), 16).green(),
                Terminal::cell(format!("{:?}", package.bump).to_lowercase(), 8),
                reason
            ));
        }
        terminal.blank();
        terminal.summary(StepOutcome::Success, &t!("cli.status.complete"));
    }

    if !warnings.is_empty() {
        terminal.warning(&t!("cli.status.pre_release_warning_header"));
    }
    for warning in warnings.iter() {
        terminal.warning(warning);
    }

    if !is_ci {
        return Ok(());
    }

    let path = env::var("GITHUB_EVENT_PATH").context("no GITHUB_EVENT_PATH")?;
    let event_data = std::fs::read_to_string(&path)?;

    log::debug!("GITHUB_EVENT_PATH: {}", path);
    log::debug!("Loaded GitHub event payload ({} bytes)", event_data.len());

    let event: GitHubEvent = serde_json::from_str(&event_data)?;

    let owner = &event.repository.owner.login;
    let head_owner = &event.pull_request.head.repo.owner.login;
    let repo_name = &event.repository.name;
    let pr_number = event.pull_request.number;
    let head_ref = event.pull_request.head.ref_name;
    let base_ref = event.pull_request.base.ref_name;

    log::debug!("owner: {}", owner);
    log::debug!("repo_name: {}", repo_name);
    log::debug!("pr_number: {}", pr_number);
    log::debug!("head_ref: {}", head_ref);
    log::debug!("base_ref: {}", base_ref);

    let octocrab = Octocrab::builder()
        .personal_token(env::var("GITHUB_TOKEN")?)
        .build()?;

    let is_matched = base_ref == config.branches.base
        && (head_ref != config.branches.base || head_owner != owner);
    if status.comment && is_matched {
        let issues = octocrab.issues(owner, repo_name);

        let comments = octocrab
            .all_pages(issues.list_comments(pr_number).send().await?)
            .await?;
        let files = octocrab
            .all_pages(
                octocrab
                    .pulls(owner, repo_name)
                    .list_files(pr_number)
                    .await?,
            )
            .await?;
        let changeset_directory = project
            .changeset_dir
            .strip_prefix(&project.root)
            .map_err(|_| anyhow::anyhow!(t!("cli.status.comment_changeset_path_invalid")))?;
        let branch_changesets = pull_request_changesets(&files, changeset_directory);
        let existing = comments.iter().find(|comment| {
            comment.user.login == "github-actions[bot]"
                && comment
                    .body
                    .as_deref()
                    .is_some_and(is_semifold_comment_body)
        });

        let comment_body = render_github_comment(&GithubCommentModel::from_plan(
            &plan,
            &event.pull_request.head.sha,
            &base_ref,
            branch_changesets,
        ));

        if let Err(error) = write_status_comment(
            &octocrab,
            owner,
            repo_name,
            pr_number,
            existing,
            comment_body,
        )
        .await
        {
            terminal.warning(&render_comment_write_error(&error));
        }
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommentOperation {
    Create,
    Update,
}

#[derive(Debug, Eq, PartialEq)]
enum GitHubCommentError {
    Api {
        status: String,
        status_code: u16,
        message: String,
        documentation_url: Option<String>,
    },
    Client {
        message: String,
    },
}

impl GitHubCommentError {
    fn from_api_error(source: &octocrab::GitHubError) -> Self {
        Self::from_api_parts(
            source.status_code.to_string(),
            source.status_code.as_u16(),
            source.message.clone(),
            source.documentation_url.clone(),
        )
    }

    fn from_api_parts(
        status: String,
        status_code: u16,
        message: String,
        documentation_url: Option<String>,
    ) -> Self {
        Self::Api {
            status,
            status_code,
            message,
            documentation_url,
        }
    }
}

impl From<octocrab::Error> for GitHubCommentError {
    fn from(error: octocrab::Error) -> Self {
        match error {
            octocrab::Error::GitHub { source, .. } => Self::from_api_error(&source),
            error => Self::Client {
                message: format_error_chain(&error),
            },
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct CommentWriteError {
    operation: CommentOperation,
    source: GitHubCommentError,
}

impl CommentWriteError {
    fn new(operation: CommentOperation, source: GitHubCommentError) -> Self {
        Self { operation, source }
    }
}

async fn write_status_comment(
    octocrab: &Octocrab,
    owner: &str,
    repository: &str,
    pull_request: u64,
    existing: Option<&Comment>,
    body: String,
) -> Result<(), CommentWriteError> {
    let (operation, result) = if let Some(comment) = existing {
        (
            CommentOperation::Update,
            update_status_comment(octocrab, owner, repository, comment, &body).await,
        )
    } else {
        (
            CommentOperation::Create,
            create_status_comment(octocrab, owner, repository, pull_request, &body).await,
        )
    };
    result.map_err(|source| CommentWriteError::new(operation, source))?;

    Ok(())
}

async fn create_status_comment(
    octocrab: &Octocrab,
    owner: &str,
    repository: &str,
    pull_request: u64,
    body: &str,
) -> Result<(), GitHubCommentError> {
    octocrab
        .issues(owner, repository)
        .create_comment(pull_request, body)
        .await?;
    Ok(())
}

async fn update_status_comment(
    octocrab: &Octocrab,
    owner: &str,
    repository: &str,
    comment: &Comment,
    body: &str,
) -> Result<(), GitHubCommentError> {
    octocrab
        .issues(owner, repository)
        .update_comment(comment.id, body)
        .await?;
    Ok(())
}

fn render_comment_write_error(error: &CommentWriteError) -> String {
    let operation = match error.operation {
        CommentOperation::Create => t!("cli.status.comment_operation_create"),
        CommentOperation::Update => t!("cli.status.comment_operation_update"),
    };
    match &error.source {
        GitHubCommentError::Client { message } => t!(
            "cli.status.comment_failed_with_error",
            operation = operation,
            error = message
        )
        .into_owned(),
        GitHubCommentError::Api {
            status,
            status_code,
            message,
            documentation_url,
        } => {
            let mut lines = vec![
                t!("cli.status.comment_failed", operation = operation).into_owned(),
                t!(
                    "cli.status.github_api_error",
                    status = status,
                    message = message
                )
                .into_owned(),
            ];
            if *status_code == 403 {
                lines.push(t!("cli.status.comment_permission_hint").into_owned());
            }
            if let Some(documentation_url) = documentation_url.as_deref() {
                lines.push(
                    t!("cli.status.github_documentation", url = documentation_url).into_owned(),
                );
            }
            lines.join("\n    ")
        }
    }
}

fn format_error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut messages = vec![error.to_string()];
    let mut source = error.source();
    while let Some(error) = source {
        let message = error.to_string();
        if messages.last() != Some(&message) {
            messages.push(message);
        }
        source = error.source();
    }
    messages.join(": ")
}

struct GithubCommentModel {
    sha: String,
    base_branch: String,
    changesets: usize,
    branch_changesets: Vec<String>,
    packages: Vec<GithubCommentPackage>,
    warnings: Vec<String>,
}

struct GithubCommentPackage {
    name: String,
    current_version: String,
    next_version: String,
    bump: String,
    reasons: String,
}

impl GithubCommentModel {
    fn from_plan(
        plan: &ReleasePlan,
        sha: &str,
        base_branch: &str,
        branch_changesets: Vec<String>,
    ) -> Self {
        Self {
            sha: sha.to_string(),
            base_branch: base_branch.to_string(),
            changesets: plan.consumed_changesets().len(),
            branch_changesets,
            packages: plan
                .packages()
                .iter()
                .map(|package| GithubCommentPackage {
                    name: package.id.as_str().to_string(),
                    current_version: package.current_version.to_string(),
                    next_version: package.next_version.to_string(),
                    bump: format!("{:?}", package.bump).to_lowercase(),
                    reasons: render_reasons(package),
                })
                .collect(),
            warnings: plan
                .warnings()
                .iter()
                .map(|warning| match warning {
                    PlanWarning::NonPatchBumpOnPrerelease { package, .. } => {
                        t!("cli.status.pre_release_warning", package = package.as_str())
                            .into_owned()
                    }
                })
                .collect(),
        }
    }
}

fn render_github_comment(model: &GithubCommentModel) -> String {
    let mut sections = vec![
        COMMENT_MARKER.to_string(),
        format!("## {}", t!("cli.status.comment_title")),
        t!("cli.status.comment_through", sha = model.sha).into_owned(),
    ];

    if model.branch_changesets.is_empty() {
        sections.push(format!(
            "> [!NOTE]\n> {}\n>\n> {}",
            t!("cli.status.comment_empty"),
            t!(
                "cli.status.comment_empty_release",
                branch = model.base_branch
            )
        ));
    } else {
        sections.push(
            t!(
                "cli.status.comment_branch_changesets",
                changesets = model
                    .branch_changesets
                    .iter()
                    .map(|changeset| format!("`{}`", markdown_cell(changeset)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into_owned(),
        );
        sections.push(
            t!(
                "cli.status.comment_summary",
                changesets = model.changesets,
                packages = model.packages.len()
            )
            .into_owned(),
        );
        let mut table = vec![
            format!(
                "| {} | {} | {} | {} | {} |",
                t!("cli.status.column_package"),
                t!("cli.status.column_current"),
                t!("cli.status.column_next"),
                t!("cli.status.column_bump"),
                t!("cli.status.column_reason")
            ),
            "| --- | --- | --- | --- | --- |".to_string(),
        ];
        table.extend(model.packages.iter().map(|package| {
            format!(
                "| `{}` | `{}` | `{}` | **{}** | {} |",
                markdown_cell(&package.name),
                markdown_cell(&package.current_version),
                markdown_cell(&package.next_version),
                markdown_cell(&package.bump),
                markdown_cell(&package.reasons)
            )
        }));
        sections.push(table.join("\n"));
    }

    if !model.warnings.is_empty() {
        sections.push(format!(
            "> [!WARNING]\n> **{}**\n{}",
            t!("cli.status.comment_warnings"),
            model
                .warnings
                .iter()
                .map(|warning| format!("> - {}", markdown_cell(warning)))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    sections.push(format!("_{}_", t!("cli.status.comment_footer")));
    sections.join("\n\n")
}

fn render_reasons(package: &PackageRelease) -> String {
    package
        .reasons
        .iter()
        .map(|reason| match reason {
            ReleaseReason::Changeset { .. } => t!("cli.status.reason_changeset"),
            ReleaseReason::DependencyPropagation { .. } => t!("cli.status.reason_dependency"),
            ReleaseReason::SharedVersionPropagation { .. } => {
                t!("cli.status.reason_shared_version")
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ")
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}

fn is_semifold_comment_body(body: &str) -> bool {
    body.contains(COMMENT_MARKER) || body.starts_with(LEGACY_COMMENT_PREFIX)
}

fn pull_request_changesets(files: &[DiffEntry], changeset_directory: &Utf8Path) -> Vec<String> {
    files
        .iter()
        .filter_map(|file| {
            pull_request_changeset_id(&file.filename, &file.status, changeset_directory)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn pull_request_changeset_id(
    filename: &str,
    status: &DiffEntryStatus,
    changeset_directory: &Utf8Path,
) -> Option<String> {
    if status == &DiffEntryStatus::Removed {
        return None;
    }
    let path = Utf8Path::new(filename);
    if path.parent() != Some(changeset_directory) || path.extension() != Some("md") {
        return None;
    }
    path.file_stem().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_error(status_code: u16) -> GitHubCommentError {
        GitHubCommentError::Api {
            status: format!("{status_code} Forbidden"),
            status_code,
            message: "Resource not accessible by integration".to_string(),
            documentation_url: Some("https://docs.github.com/rest/issues/comments".to_string()),
        }
    }

    #[test]
    fn empty_github_comment_explains_post_merge_publishing() {
        let body = render_github_comment(&GithubCommentModel {
            sha: "abc123".to_string(),
            base_branch: "main".to_string(),
            changesets: 0,
            branch_changesets: Vec::new(),
            packages: Vec::new(),
            warnings: Vec::new(),
        });

        assert!(body.starts_with(COMMENT_MARKER));
        assert!(body.contains("Plan calculated through commit abc123."));
        assert!(!body.contains("`abc123`"));
        assert!(body.contains("> [!NOTE]"));
        assert!(body.contains("`main`"));
        assert!(!body.contains("| --- |"));
    }

    #[test]
    fn populated_github_comment_keeps_release_details_visible() {
        let body = render_github_comment(&GithubCommentModel {
            sha: "abc123".to_string(),
            base_branch: "main".to_string(),
            changesets: 2,
            branch_changesets: vec!["feature".to_string()],
            packages: vec![GithubCommentPackage {
                name: "app".to_string(),
                current_version: "1.0.0".to_string(),
                next_version: "1.1.0".to_string(),
                bump: "minor".to_string(),
                reasons: "changeset".to_string(),
            }],
            warnings: vec!["check prerelease".to_string()],
        });

        assert!(body.contains("| `app` | `1.0.0` | `1.1.0` | **minor** | changeset |"));
        assert!(body.contains("> [!WARNING]"));
        assert!(!body.contains("<details>"));
    }

    #[test]
    fn markdown_cells_cannot_break_the_table() {
        assert_eq!(markdown_cell("a|b\nc"), "a\\|b c");
    }

    #[test]
    fn comment_marker_does_not_claim_unrelated_bot_comments() {
        assert!(is_semifold_comment_body(&format!(
            "{COMMENT_MARKER}\n## release"
        )));
        assert!(is_semifold_comment_body(
            "## Workspace change through: abc123"
        ));
        assert!(!is_semifold_comment_body("## Coverage report"));
    }

    #[test]
    fn pull_request_changeset_detection_respects_path_extension_and_status() {
        let directory = Utf8Path::new(".changes");
        assert_eq!(
            pull_request_changeset_id(".changes/feature.md", &DiffEntryStatus::Added, directory)
                .as_deref(),
            Some("feature")
        );
        assert_eq!(
            pull_request_changeset_id(".changes/fix.md", &DiffEntryStatus::Modified, directory)
                .as_deref(),
            Some("fix")
        );
        assert!(
            pull_request_changeset_id(".changes/removed.md", &DiffEntryStatus::Removed, directory)
                .is_none()
        );
        assert!(
            pull_request_changeset_id(
                ".changes/config.toml",
                &DiffEntryStatus::Modified,
                directory
            )
            .is_none()
        );
        assert!(
            pull_request_changeset_id(
                ".changes/nested/feature.md",
                &DiffEntryStatus::Added,
                directory
            )
            .is_none()
        );
    }

    #[test]
    fn github_comment_warning_includes_api_details_and_permission_guidance() {
        let rendered = render_comment_write_error(&CommentWriteError {
            operation: CommentOperation::Create,
            source: api_error(403),
        });

        assert_eq!(
            rendered,
            concat!(
                "Failed to create the release plan pull request comment.\n",
                "    Error: GitHub API returned 403 Forbidden: ",
                "Resource not accessible by integration\n",
                "    Hint: Check the workflow token's issues/pull-requests permissions ",
                "and whether this is a fork pull request.\n",
                "    GitHub documentation: ",
                "https://docs.github.com/rest/issues/comments"
            )
        );
        assert!(!rendered.contains("Authorization"));
    }

    #[test]
    fn github_comment_warning_distinguishes_update_without_overstating_non_403_errors() {
        let rendered = render_comment_write_error(&CommentWriteError {
            operation: CommentOperation::Update,
            source: GitHubCommentError::Api {
                status: "500 Internal Server Error".to_string(),
                status_code: 500,
                message: "Server Error".to_string(),
                documentation_url: None,
            },
        });

        assert_eq!(
            rendered,
            concat!(
                "Failed to update the release plan pull request comment.\n",
                "    Error: GitHub API returned 500 Internal Server Error: Server Error"
            )
        );
    }

    #[test]
    fn github_comment_client_error_stays_on_one_line() {
        let rendered = render_comment_write_error(&CommentWriteError {
            operation: CommentOperation::Create,
            source: GitHubCommentError::Client {
                message: "request timed out: connection closed".to_string(),
            },
        });

        assert_eq!(
            rendered,
            "Failed to create the release plan pull request comment: request timed out: connection closed"
        );
        assert!(!rendered.contains('\n'));
    }

    #[test]
    fn github_api_error_preserves_structured_details() {
        let converted = GitHubCommentError::from_api_parts(
            reqwest::StatusCode::FORBIDDEN.to_string(),
            reqwest::StatusCode::FORBIDDEN.as_u16(),
            "Resource not accessible by integration".to_string(),
            Some("https://docs.github.com/rest/issues/comments".to_string()),
        );

        assert_eq!(converted, api_error(403));
    }
}
