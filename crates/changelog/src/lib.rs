#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use semifold_resolver::{changeset, context, error::ResolveError};

use crate::utils::PrInfo;

pub mod types;
pub mod utils;

pub struct GeneratedChangelog {
    pub content: String,
    pub remote_metadata_failed: bool,
}

/// Immutable, fully collected input for pure changelog Markdown formatting.
pub struct ChangelogContext {
    pub package_version: String,
    pub sections: BTreeMap<String, Vec<String>>,
    pub dependency_updates: Vec<(String, String)>,
}

pub fn format_changelog(context: &ChangelogContext) -> String {
    let header = format!("## v{}\n\n", context.package_version);
    let changes_body = context
        .sections
        .iter()
        .map(|(tag, lines)| format!("### {tag}\n\n{}", lines.join("\n")))
        .collect::<Vec<_>>()
        .join("\n\n");
    let dependencies_body = format_dependency_updates(&context.dependency_updates);
    let body = [changes_body, dependencies_body]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    header + &body
}

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
            "{}/{}/{}/commit/{}",
            repo_info.base_url, repo_info.owner, repo_info.repo_name, commit_hash
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
    dependency_updates: &[(String, String)],
    collect_remote_metadata: bool,
) -> Result<GeneratedChangelog, ResolveError> {
    let mut changes_map = BTreeMap::new();
    let mut remote_metadata_failed = false;

    let tags = ctx
        .config
        .as_ref()
        .map(|c| c.tags.clone())
        .unwrap_or_default();

    for changeset in changesets {
        let changeset_path = changeset
            .path
            .as_ref()
            .ok_or(ResolveError::InvalidChangeset {
                path: PathBuf::new(),
                reason: "Changeset is missing its source path".to_string(),
            })?;
        let repo_root = ctx.repo_root.as_ref().ok_or(ResolveError::GitError {
            message: "Repository root is not available".to_string(),
        })?;
        let rel_path = pathdiff::diff_paths(changeset_path, repo_root).ok_or(
            ResolveError::InvalidChangeset {
                path: changeset_path.to_path_buf(),
                reason: "Changeset path is not under repo root".to_string(),
            },
        )?;
        let commit_info = utils::find_first_commit_for_path(repo, &rel_path);
        let commit_hash = commit_info.as_ref().map(|c| c.oid.to_string());
        let pr_info = if collect_remote_metadata
            && let Some(repo_info) = ctx.repo_info.as_ref()
            && let Some(commit_info) = commit_info.as_ref()
        {
            match utils::query_pr_for_commit(
                repo_info.owner.as_str(),
                repo_info.repo_name.as_str(),
                commit_info,
            )
            .await
            {
                Ok(pr_info) => pr_info,
                Err(error) => {
                    eprintln!("{error:?}");
                    remote_metadata_failed = true;
                    None
                }
            }
        } else {
            None
        };

        let package = changeset.packages.iter().find(|p| p.name == package_name);
        if let Some(package) = package {
            let tag = package
                .tag
                .as_ref()
                .and_then(|t| tags.get(t).map(|s| s.as_str()))
                .unwrap_or("Changes")
                .to_string();
            changes_map
                .entry(tag)
                .or_insert_with(Vec::new)
                .push(format_line(
                    changeset,
                    &ctx.repo_info,
                    &pr_info,
                    &commit_hash,
                ));
        }
    }

    Ok(GeneratedChangelog {
        content: format_changelog(&ChangelogContext {
            package_version: package_version.to_string(),
            sections: changes_map,
            dependency_updates: dependency_updates.to_vec(),
        }),
        remote_metadata_failed,
    })
}

pub fn format_dependency_updates(dependency_updates: &[(String, String)]) -> String {
    if dependency_updates.is_empty() {
        return String::new();
    }
    let lines = dependency_updates
        .iter()
        .map(|(dependency, version)| format!("- Update {dependency} to {version}."))
        .collect::<Vec<_>>();
    format!("### Dependencies\n\n{}", lines.join("\n"))
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use std::collections::BTreeMap;

    use super::{
        ChangelogContext, format_changelog, format_dependency_updates, utils::render_changelog,
    };

    #[test]
    fn formats_an_immutable_context_without_external_clients() {
        let context = ChangelogContext {
            package_version: "1.0.0".to_string(),
            sections: BTreeMap::from([(
                "Changes".to_string(),
                vec!["- Add release planning".to_string()],
            )]),
            dependency_updates: vec![("core".to_string(), "1.0.0".to_string())],
        };

        assert_eq!(
            format_changelog(&context),
            "## v1.0.0\n\n### Changes\n\n- Add release planning\n\n### Dependencies\n\n- Update core to 1.0.0."
        );
    }

    #[test]
    fn formats_propagated_dependency_updates_as_a_separate_section() {
        assert_eq!(
            format_dependency_updates(&[(
                "semifold-resolver".to_string(),
                "0.4.0-alpha.0".to_string(),
            )]),
            "### Dependencies\n\n- Update semifold-resolver to 0.4.0-alpha.0."
        );
    }

    #[test]
    fn renders_a_new_or_existing_changelog_without_writing() {
        assert_eq!(
            render_changelog(
                Path::new("CHANGELOG.md"),
                None,
                "## v1.0.0\n\n### Changes\n\n- Add"
            )
            .unwrap(),
            "# Changelog\n\n## v1.0.0\n\n### Changes\n\n- Add\n"
        );
        assert_eq!(
            render_changelog(
                Path::new("CHANGELOG.md"),
                Some("# Changelog\n\n## v0.1.0\n\n- Old\n"),
                "## v1.0.0\n\n### Changes\n\n- Add"
            )
            .unwrap(),
            "# Changelog\n\n## v1.0.0\n\n### Changes\n\n- Add\n\n## v0.1.0\n\n- Old\n"
        );
    }
}
