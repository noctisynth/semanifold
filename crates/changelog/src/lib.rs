use std::collections::HashMap;

use semifold_resolver::{changeset, context, error::ResolveError};

use crate::utils::PrInfo;

pub mod utils;

pub fn format_line(
    changeset: &changeset::Changeset,
    owner: &str,
    repo_name: &str,
    pr_info: &Option<PrInfo>,
    commit_hash: &str,
) -> String {
    let mut line = String::from("- ");

    let commit_url = format!(
        "https://github.com/{}/{}/commit/{}",
        owner, repo_name, commit_hash
    );
    line.push_str(&format!(" [`{}`]({}): ", &commit_hash[..7], commit_url));
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
    owner: &str,
    repo_name: &str,
    repo: &git2::Repository,
    changesets: &[changeset::Changeset],
    package_name: &str,
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
        let commit_info =
            utils::find_first_commit_for_path(repo, &rel_path).ok_or(ResolveError::GitError {
                message: format!("Failed to find commit for path: {:?}", rel_path),
            })?;
        let commit_hash = commit_info.oid.to_string();
        let pr_info = utils::query_pr_for_commit(owner, repo_name, &commit_info)
            .await
            .map_err(|e| ResolveError::GitHubError {
                message: format!("Failed to query PR for commit: {:?}", e),
            })?;

        let package = changeset.packages.iter().find(|p| p.name == package_name);
        if let Some(package) = package {
            changes_map
                .entry(tags.get(&package.tag).map_or("Changes", |v| v))
                .or_insert_with(Vec::new)
                .push(format_line(
                    changeset,
                    owner,
                    repo_name,
                    &pr_info,
                    &commit_hash,
                ));
        }
    }

    Ok(changes_map
        .iter()
        .map(|(tag, lines)| format!("### {tag}\n\n{}", lines.join("\n")))
        .collect::<Vec<_>>()
        .join("\n\n"))
}
