use std::{collections::HashMap, path::Path};

use semifold_resolver::{changeset, context, error::ResolveError};

use crate::utils::PrInfo;

pub mod types;
pub mod utils;

pub fn format_line(
    changeset: &changeset::Changeset,
    repo_info: &Option<context::RepoInfo>,
    pr_info: &Option<PrInfo>,
    commit_hash: &Option<String>,
) -> String {
    let mut line = String::from("- ");

    if let Some(repo_info) = repo_info.as_ref()
        && let Some(commit_hash) = commit_hash
    {
        let commit_url = format!(
            "https://github.com/{}/{}/commit/{}",
            repo_info.owner, repo_info.repo_name, commit_hash
        );
        line.push_str(&format!("[`{}`]({}): ", &commit_hash[..7], commit_url));
    }
    line.push_str(&changeset.summary);

    if let Some(pr_info) = pr_info.as_ref() {
        if let Some(url) = pr_info.url.as_ref() {
            line.push_str(&format!(" ([#{}]({url})", pr_info.number));
        } else {
            line.push_str(&format!(" (#{}", pr_info.number));
        }
        if let Some(author) = pr_info.author.as_ref() {
            line.push_str(&format!(" by @{}", author));
        }
        line.push(')');
    }

    line
}

pub async fn generate_changelog(
    ctx: &context::Context,
    repo: &git2::Repository,
    changesets: &[changeset::Changeset],
    package_name: &str,
    package_version: &str,
) -> Result<String, ResolveError> {
    let mut changes_map = HashMap::new();

    let tags = ctx
        .config
        .as_ref()
        .map(|c| c.tags.clone())
        .unwrap_or_default();

    for changeset in changesets {
        let changeset_path = changeset.path.as_ref().unwrap();
        let rel_path = pathdiff::diff_paths(changeset_path, ctx.repo_root.as_ref().unwrap())
            .ok_or(ResolveError::InvalidChangeset {
                path: changeset_path.to_path_buf(),
                reason: "Changeset path is not under repo root".to_string(),
            })?;
        let commit_info = utils::find_first_commit_for_path(repo, &rel_path);
        let commit_hash = commit_info.as_ref().map(|c| c.oid.to_string());
        let pr_info = if let Some(repo_info) = ctx.repo_info.as_ref()
            && let Some(commit_info) = commit_info.as_ref()
        {
            utils::query_pr_for_commit(
                repo_info.owner.as_str(),
                repo_info.repo_name.as_str(),
                commit_info,
            )
            .await
            .map_err(|e| ResolveError::GitHubError {
                message: format!("Failed to query PR for commit: {:?}", e),
            })?
        } else {
            None
        };

        let package = changeset.packages.iter().find(|p| p.name == package_name);
        if let Some(package) = package {
            changes_map
                .entry(tags.get(&package.tag).map_or("Changes", |v| v))
                .or_insert_with(Vec::new)
                .push(format_line(
                    changeset,
                    &ctx.repo_info,
                    &pr_info,
                    &commit_hash,
                ));
        }
    }

    let header = format!("## v{package_version}\n\n");
    let body = changes_map
        .iter()
        .map(|(tag, lines)| format!("### {tag}\n\n{}", lines.join("\n")))
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok(header + &body)
}

pub async fn read_latest_changelog<P: AsRef<Path>>(
    path: P,
) -> Result<types::Changelog, ResolveError> {
    let content = std::fs::read_to_string(path.as_ref())?;

    let mut lines = content.lines();

    if lines.next().map(|l| l.trim()) != Some("# Changelog") {
        return Err(ResolveError::InvalidChangelog {
            path: path.as_ref().to_path_buf(),
            reason: "Invalid changelog: missing `# Changelog` header".to_string(),
        });
    }

    let mut version: Option<String> = None;
    let mut body = String::new();
    let mut in_latest = false;

    for line in content.lines().skip(1) {
        let trimmed = line.trim();

        if version.is_none() {
            if let Some(rest) = trimmed.strip_prefix("## ") {
                version = Some(rest.to_string());
                in_latest = true;

                body.push_str(line);
                body.push('\n');
                continue;
            }
        } else if in_latest && trimmed.starts_with("## ") {
            break;
        } else if in_latest {
            body.push_str(line);
            body.push('\n');
        }
    }

    let version = version.ok_or(ResolveError::InvalidChangelog {
        path: path.as_ref().to_path_buf(),
        reason: "No version header found".to_string(),
    })?;

    Ok(types::Changelog {
        version,
        body: body.trim().to_string(),
    })
}
