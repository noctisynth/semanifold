#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub use semifold_core::{
    ChangelogContext, ChangesetContext, DependencyUpdateContext, PackageChangesetContext,
};
use semifold_core::{ChangesetId, CommitContext, PullRequestContext, ReleasePackageContext};
use semifold_resolver::{changeset, context, error::ResolveError};

pub mod types;
pub mod utils;

pub struct GeneratedChangelog {
    pub content: String,
    pub remote_metadata_failed: bool,
}

pub struct CollectedChangelogContext<'release> {
    pub context: ChangelogContext<'release>,
    pub remote_metadata_failed: bool,
}

pub fn format_changelog(context: &ChangelogContext<'_>) -> String {
    let header = format!("## v{}\n\n", context.package.package.next_version);
    let sections = context.changesets.iter().fold(
        BTreeMap::<&str, Vec<String>>::new(),
        |mut sections, package_changeset| {
            sections
                .entry(&package_changeset.section)
                .or_default()
                .push(format_line(&package_changeset.changeset));
            sections
        },
    );
    let changes_body = sections
        .iter()
        .map(|(section, lines)| format!("### {section}\n\n{}", lines.join("\n")))
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

pub fn format_line(changeset: &ChangesetContext) -> String {
    let mut line = String::from("- ");

    if let Some(commit) = changeset.commit.as_ref()
        && let Some(commit_url) = commit.web_url.as_ref()
    {
        let short_sha = commit.sha.chars().take(7).collect::<String>();
        line.push_str(&format!("[`{short_sha}`]({commit_url}): "));
    }

    let summary_paragraphs =
        changeset
            .summary
            .lines()
            .fold(Vec::<Vec<&str>>::new(), |mut paragraphs, summary_line| {
                if summary_line.trim().is_empty() {
                    if paragraphs
                        .last()
                        .is_some_and(|paragraph| !paragraph.is_empty())
                    {
                        paragraphs.push(Vec::new());
                    }
                } else if let Some(paragraph) = paragraphs.last_mut() {
                    paragraph.push(summary_line);
                } else {
                    paragraphs.push(vec![summary_line]);
                }
                paragraphs
            });
    if let Some(first_line) = summary_paragraphs
        .first()
        .and_then(|paragraph| paragraph.first())
    {
        line.push_str(first_line);
    }

    if let Some(pull_request) = changeset.pull_request.as_ref() {
        if let Some(url) = pull_request.web_url.as_ref() {
            line.push_str(&format!(" ([#{}]({url})", pull_request.number));
        } else {
            line.push_str(&format!(" (#{}", pull_request.number));
        }
        if let Some(author) = pull_request.author.as_ref() {
            line.push_str(&format!(" by @{}", author));
        }
        line.push(')');
    }

    let mut has_continuation = false;
    for (paragraph_index, paragraph) in summary_paragraphs.iter().enumerate() {
        let first_line_index = usize::from(paragraph_index == 0);
        for (line_index, summary_line) in paragraph.iter().enumerate().skip(first_line_index) {
            if line_index == 0 {
                line.push_str("\n\n    ");
            } else {
                line.push_str("\n    ");
            }
            line.push_str(summary_line);
            has_continuation = true;
        }
    }
    if has_continuation {
        line.push('\n');
    }

    line
}

pub async fn collect_changelog_context<'release>(
    ctx: &context::Context,
    repo: &git2::Repository,
    changesets: &[changeset::Changeset],
    package: ReleasePackageContext<'release>,
    dependency_updates: Vec<DependencyUpdateContext>,
    collect_remote_metadata: bool,
) -> Result<CollectedChangelogContext<'release>, ResolveError> {
    let mut collected_changesets = Vec::new();
    let mut remote_metadata_failed = false;

    let tags = ctx
        .config
        .as_ref()
        .map(|c| c.tags.clone())
        .unwrap_or_default();

    for changeset in changesets {
        let Some(changed_package) = changeset
            .packages
            .iter()
            .find(|changed| changed.name == package.package.id.as_str())
        else {
            continue;
        };
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
                Err(_error) => {
                    remote_metadata_failed = true;
                    None
                }
            }
        } else {
            None
        };

        let commit = commit_info.map(|commit| {
            let sha = commit.oid.to_string();
            let web_url = ctx.repo_info.as_ref().map(|repo_info| {
                format!(
                    "{}/{}/{}/commit/{sha}",
                    repo_info.base_url, repo_info.owner, repo_info.repo_name
                )
            });
            CommitContext { sha, web_url }
        });
        let pull_request = pr_info.map(|pull_request| PullRequestContext {
            number: pull_request.number,
            author: pull_request.author,
            web_url: pull_request.url,
        });
        let section = changed_package
            .tag
            .as_ref()
            .and_then(|tag| tags.get(tag))
            .cloned()
            .unwrap_or_else(|| "Changes".to_string());
        collected_changesets.push(PackageChangesetContext {
            changeset: ChangesetContext {
                id: ChangesetId::new(&changeset.name),
                summary: changeset.summary.clone(),
                commit,
                pull_request,
            },
            section,
        });
    }
    collected_changesets.sort_by(|left, right| left.changeset.id.cmp(&right.changeset.id));

    Ok(CollectedChangelogContext {
        context: ChangelogContext {
            package,
            changesets: collected_changesets,
            dependency_updates,
        },
        remote_metadata_failed,
    })
}

pub async fn generate_changelog<'release>(
    ctx: &context::Context,
    repo: &git2::Repository,
    changesets: &[changeset::Changeset],
    package: ReleasePackageContext<'release>,
    dependency_updates: Vec<DependencyUpdateContext>,
    collect_remote_metadata: bool,
) -> Result<GeneratedChangelog, ResolveError> {
    let collected = collect_changelog_context(
        ctx,
        repo,
        changesets,
        package,
        dependency_updates,
        collect_remote_metadata,
    )
    .await?;
    Ok(GeneratedChangelog {
        content: format_changelog(&collected.context),
        remote_metadata_failed: collected.remote_metadata_failed,
    })
}

pub fn format_dependency_updates(dependency_updates: &[DependencyUpdateContext]) -> String {
    if dependency_updates.is_empty() {
        return String::new();
    }
    let lines = dependency_updates
        .iter()
        .map(|dependency| {
            format!(
                "- Update {} to {}.",
                dependency.package, dependency.next_version
            )
        })
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

    use semifold_core::{
        BumpLevel, ChangesetId, CommitContext, DependencyUpdateContext, Ecosystem, PackageId,
        PackageRelease, PackageSnapshot, PullRequestContext, ReleaseContext, ReleasePackageContext,
        ReleasePlan, ReleaseReason, VersionMap, VersionSource,
    };
    use semver::Version;

    use super::{
        ChangelogContext, ChangesetContext, PackageChangesetContext, format_changelog,
        format_dependency_updates, format_line, utils::render_changelog,
    };

    fn release_context() -> ReleaseContext {
        let id = PackageId::new("package");
        let release = PackageRelease {
            id: id.clone(),
            ecosystem: Ecosystem::Rust,
            current_version: Version::new(0, 9, 0),
            next_version: Version::new(1, 0, 0),
            bump: BumpLevel::Major,
            reasons: vec![ReleaseReason::Changeset {
                changeset: ChangesetId::new("release"),
            }],
        };
        ReleaseContext::from_plan(
            &ReleasePlan::new(
                vec![release],
                VersionMap::from([(id.clone(), Version::new(1, 0, 0))]),
                vec![id],
                vec![ChangesetId::new("release")],
                Vec::new(),
                Vec::new(),
            )
            .unwrap(),
        )
    }

    fn package_context(release: &ReleaseContext) -> ReleasePackageContext<'_> {
        ReleasePackageContext::from_snapshot(
            release,
            &PackageSnapshot {
                id: PackageId::new("package"),
                manifest_name: "package".to_string(),
                version: Version::new(0, 9, 0),
                version_source: VersionSource::PackageManifest,
                ecosystem: Ecosystem::Rust,
                path: "crates/package".into(),
                publishable: true,
                dependencies: Vec::new(),
            },
        )
        .unwrap()
    }

    fn changeset(id: &str, summary: &str) -> ChangesetContext {
        ChangesetContext {
            id: ChangesetId::new(id),
            summary: summary.to_string(),
            commit: None,
            pull_request: None,
        }
    }

    #[test]
    fn formats_an_immutable_context_without_external_clients() {
        let release = release_context();
        let context = ChangelogContext {
            package: package_context(&release),
            changesets: vec![PackageChangesetContext {
                changeset: changeset("release", "Add release planning"),
                section: "Changes".to_string(),
            }],
            dependency_updates: vec![DependencyUpdateContext {
                package: PackageId::new("core"),
                next_version: Version::new(1, 0, 0),
            }],
        };

        assert_eq!(
            format_changelog(&context),
            "## v1.0.0\n\n### Changes\n\n- Add release planning\n\n### Dependencies\n\n- Update core to 1.0.0."
        );
    }

    #[test]
    fn formats_multiline_changesets_as_separate_list_item_paragraphs() {
        let release = release_context();
        let context = ChangelogContext {
            package: package_context(&release),
            changesets: vec![
                PackageChangesetContext {
                    changeset: changeset("first", "First line\n\nSecond line\nThird line"),
                    section: "Changes".to_string(),
                },
                PackageChangesetContext {
                    changeset: changeset("second", "Another changeset"),
                    section: "Changes".to_string(),
                },
            ],
            dependency_updates: vec![],
        };

        assert_eq!(
            format_changelog(&context),
            "## v1.0.0\n\n### Changes\n\n- First line\n\n    Second line\n    Third line\n\n- Another changeset"
        );
    }

    #[test]
    fn ignores_source_blank_lines_without_emitting_whitespace_only_paragraphs() {
        let changeset = changeset(
            "realistic-multiline",
            "Keep resume item columns within predictable bounds\n\nThe default template now gives job titles, organizations, and dates independent grid columns so\nlong content wraps without displacing adjacent fields. Linked titles no longer include trailing\nunderline space, and a complete Chinese sample covers long-title wrapping and all resume sections.",
        );

        assert_eq!(
            format_line(&changeset),
            "- Keep resume item columns within predictable bounds\n\n    The default template now gives job titles, organizations, and dates independent grid columns so\n    long content wraps without displacing adjacent fields. Linked titles no longer include trailing\n    underline space, and a complete Chinese sample covers long-title wrapping and all resume sections.\n"
        );
    }

    #[test]
    fn attaches_metadata_to_the_first_line_of_a_multiline_changeset() {
        let mut changeset = changeset("multiline", "First line\r\nSecond line\r\nThird line");
        changeset.commit = Some(CommitContext {
            sha: "1234567890abcdef".to_string(),
            web_url: Some(
                "https://github.com/semifold/semifold/commit/1234567890abcdef".to_string(),
            ),
        });
        changeset.pull_request = Some(PullRequestContext {
            number: 42,
            author: Some("author".to_string()),
            web_url: Some("https://github.com/semifold/semifold/pull/42".to_string()),
        });

        assert_eq!(
            format_line(&changeset),
            "- [`1234567`](https://github.com/semifold/semifold/commit/1234567890abcdef): First line ([#42](https://github.com/semifold/semifold/pull/42) by @author)\n    Second line\n    Third line\n"
        );
    }

    #[test]
    fn formats_propagated_dependency_updates_as_a_separate_section() {
        assert_eq!(
            format_dependency_updates(&[DependencyUpdateContext {
                package: PackageId::new("semifold-resolver"),
                next_version: Version::parse("0.4.0-alpha.0").unwrap(),
            }]),
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

    #[test]
    fn preserves_a_multiline_entries_trailing_blank_line_when_inserting() {
        assert_eq!(
            render_changelog(
                Path::new("CHANGELOG.md"),
                None,
                "## v1.0.0\n\n### Changes\n\n- First line\n\n    Second line\n"
            )
            .unwrap(),
            "# Changelog\n\n## v1.0.0\n\n### Changes\n\n- First line\n\n    Second line\n\n"
        );
    }
}
