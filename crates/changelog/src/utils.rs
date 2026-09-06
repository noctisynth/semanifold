use std::path::Path;

use git2::{DiffOptions, Oid, Repository};
use octocrab::Octocrab;

use semifold_resolver::error::ResolveError;

use crate::github::{GitHubFailure, GitHubOperation};
use crate::{RELEASE_MARKER_END, RELEASE_MARKER_PREFIX};

#[derive(Debug)]
pub struct CommitInfo {
    pub oid: Oid,
    pub author: Option<String>,
    pub message: String,
}

#[derive(Debug)]
pub struct PrInfo {
    pub number: u64,
    pub author: Option<String>,
    pub url: Option<String>,
}

pub async fn query_pr_for_commit(
    owner: &str,
    repo: &str,
    commit_info: &CommitInfo,
) -> Result<Option<PrInfo>, GitHubFailure> {
    let octocrab = if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        Octocrab::builder()
            .personal_token(token)
            .build()
            .map_err(|error| GitHubFailure::new(GitHubOperation::Initialize, error))?
    } else {
        Octocrab::builder()
            .build()
            .map_err(|error| GitHubFailure::new(GitHubOperation::Initialize, error))?
    };

    let prs = octocrab
        .repos(owner, repo)
        .list_pulls(commit_info.oid.to_string())
        .send()
        .await
        .map_err(|error| GitHubFailure::new(GitHubOperation::QueryCommitPullRequest, error))?;

    if let Some(pr) = prs.items.into_iter().next() {
        return Ok(Some(PrInfo {
            number: pr.number,
            author: pr.user.map(|u| u.login),
            url: pr.html_url.map(|u| u.to_string()),
        }));
    }

    if let Some(pr_number) = pull_request_number(&commit_info.message) {
        let pr = octocrab
            .pulls(owner, repo)
            .get(pr_number)
            .await
            .map_err(|error| GitHubFailure::new(GitHubOperation::QueryCommitPullRequest, error))?;
        return Ok(Some(PrInfo {
            number: pr.number,
            author: pr.user.map(|u| u.login),
            url: pr.html_url.map(|u| u.to_string()),
        }));
    }

    Ok(None)
}

fn pull_request_number(message: &str) -> Option<u64> {
    message.match_indices("(#").find_map(|(start, _)| {
        let remainder = message.get(start + 2..)?;
        let digits = remainder
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        if digits.is_empty()
            || !remainder
                .get(digits.len()..)
                .is_some_and(|suffix| suffix.starts_with(')'))
        {
            return None;
        }
        digits.parse().ok()
    })
}

pub fn find_first_commit_for_path(repo: &Repository, path: &Path) -> Option<CommitInfo> {
    let mut revwalk = repo.revwalk().ok()?;
    revwalk.push_head().ok()?;
    revwalk
        .set_sorting(git2::Sort::TIME | git2::Sort::REVERSE)
        .ok()?;

    for oid in revwalk {
        let oid = oid.ok()?;
        let commit = repo.find_commit(oid).ok()?;
        let tree = commit.tree().ok()?;

        if commit.parent_count() == 0 {
            if tree.get_path(std::path::Path::new(path)).is_ok() {
                return Some(CommitInfo {
                    oid,
                    author: commit.author().name().ok().map(ToOwned::to_owned),
                    message: commit.message().ok()?.to_string(),
                });
            }
        } else {
            let parent = commit.parent(0).ok()?;
            let parent_tree = parent.tree().ok()?;

            let mut diff_opts = DiffOptions::new();
            diff_opts.pathspec(path);

            let diff = repo
                .diff_tree_to_tree(Some(&parent_tree), Some(&tree), Some(&mut diff_opts))
                .ok()?;

            if diff.deltas().len() > 0 {
                return Some(CommitInfo {
                    oid,
                    author: commit.author().name().ok().map(ToOwned::to_owned),
                    message: commit.message().ok()?.to_string(),
                });
            }
        }
    }
    None
}

/// Renders the complete changelog content after inserting one newest entry.
pub fn render_changelog<P: AsRef<Path>>(
    path: P,
    content: Option<&str>,
    new_entry: &str,
    version: &str,
    require_marker: bool,
) -> Result<String, ResolveError> {
    let path = path.as_ref();
    let header = "# Changelog";

    let content = content
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{header}\n\n"));
    if content.lines().next() != Some(header) {
        return Err(ResolveError::InvalidChangelog {
            path: path.to_path_buf(),
            reason: "Invalid changelog: missing `# Changelog` root header".to_string(),
        });
    }

    let header_positions = content
        .match_indices(header)
        .filter(|(index, _)| {
            let before_is_boundary = *index == 0
                || content
                    .as_bytes()
                    .get(index.saturating_sub(1))
                    .is_some_and(|byte| *byte == b'\n');
            let after_index = index + header.len();
            let after_is_boundary = content
                .as_bytes()
                .get(after_index)
                .is_none_or(|byte| *byte == b'\n' || *byte == b'\r');
            before_is_boundary && after_is_boundary
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let Some(insert_pos) = header_positions.first().copied() else {
        return Err(ResolveError::InvalidChangelog {
            path: path.to_path_buf(),
            reason: "No `# Changelog` header found in file".to_string(),
        });
    };
    if header_positions.len() != 1 {
        return Err(ResolveError::InvalidChangelog {
            path: path.to_path_buf(),
            reason: "Multiple `# Changelog` headers found in file".to_string(),
        });
    }
    let has_markers = validate_release_markers(path, &content)?;
    let use_marker = require_marker || has_markers;

    let after_header_pos = insert_pos + header.len();
    let before = &content[..after_header_pos].trim_end_matches('\n');
    let after = &content[after_header_pos..].trim_start_matches('\n');
    let new_entry = if use_marker {
        let version =
            semver::Version::parse(version).map_err(|error| ResolveError::InvalidChangelog {
                path: path.to_path_buf(),
                reason: format!("Invalid release marker version {version}: {error}"),
            })?;
        format!(
            "{RELEASE_MARKER_PREFIX}{version} -->\n{}\n{RELEASE_MARKER_END}",
            new_entry.trim_matches('\n')
        )
    } else {
        new_entry.trim_start().to_string()
    };

    let mut new_content = String::with_capacity(content.len() + new_entry.len() + 4);
    new_content.push_str(before);
    new_content.push_str("\n\n");
    new_content.push_str(&new_entry);
    if !after.is_empty() {
        if new_entry.ends_with('\n') {
            new_content.push('\n');
        } else {
            new_content.push_str("\n\n");
        }
        new_content.push_str(after.trim_end_matches('\n'));
    }
    new_content.push('\n');

    Ok(new_content)
}

pub(crate) fn validate_release_markers(path: &Path, content: &str) -> Result<bool, ResolveError> {
    let mut open = false;
    let mut found = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(version) = release_marker_version(trimmed) {
            if open {
                return Err(invalid_marker(path, "Nested release marker"));
            }
            semver::Version::parse(version).map_err(|error| {
                invalid_marker(
                    path,
                    &format!("Invalid release marker version {version}: {error}"),
                )
            })?;
            open = true;
            found = true;
        } else if trimmed == RELEASE_MARKER_END {
            if !open {
                return Err(invalid_marker(
                    path,
                    "Release end marker has no matching start",
                ));
            }
            open = false;
        } else if trimmed.starts_with("<!-- semifold:release") {
            return Err(invalid_marker(path, "Malformed release marker"));
        }
    }
    if open {
        return Err(invalid_marker(
            path,
            "Release start marker has no matching end",
        ));
    }
    Ok(found)
}

pub(crate) fn release_marker_version(line: &str) -> Option<&str> {
    line.strip_prefix(RELEASE_MARKER_PREFIX)
        .and_then(|value| value.strip_suffix(" -->"))
}

fn invalid_marker(path: &Path, reason: &str) -> ResolveError {
    ResolveError::InvalidChangelog {
        path: path.to_path_buf(),
        reason: reason.to_string(),
    }
}

pub async fn insert_changelog<P: AsRef<Path>>(
    path: P,
    new_entry: &str,
    version: &str,
    require_marker: bool,
) -> Result<(), ResolveError> {
    let path = path.as_ref();
    let content = if path.exists() {
        Some(std::fs::read_to_string(path)?)
    } else {
        None
    };
    let new_content =
        render_changelog(path, content.as_deref(), new_entry, version, require_marker)?;
    std::fs::write(path, new_content)?;
    Ok(())
}
