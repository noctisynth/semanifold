use std::path::Path;

use git2::{DiffOptions, Oid, Repository};
use octocrab::Octocrab;

use regex::Regex;

#[derive(Debug)]
pub struct CommitInfo {
    pub oid: Oid,
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
) -> octocrab::Result<Option<PrInfo>> {
    let octocrab = Octocrab::builder().build()?;

    let prs = octocrab
        .repos(owner, repo)
        .list_pulls(commit_info.oid.to_string())
        .send()
        .await?;

    if let Some(pr) = prs.items.into_iter().next() {
        return Ok(Some(PrInfo {
            number: pr.number,
            author: pr.user.map(|u| u.login),
            url: pr.html_url.map(|u| u.to_string()),
        }));
    }

    let re = Regex::new(r"\(#(\d+)\)").unwrap();
    if let Some(caps) = re.captures(&commit_info.message)
        && let Ok(pr_number) = caps[1].parse::<u64>()
    {
        let pr = octocrab.pulls(owner, repo).get(pr_number).await?;
        return Ok(Some(PrInfo {
            number: pr.number,
            author: pr.user.map(|u| u.login),
            url: pr.html_url.map(|u| u.to_string()),
        }));
    }

    Ok(None)
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
                    message: commit.message()?.to_string(),
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
                    message: commit.message()?.to_string(),
                });
            }
        }
    }
    None
}
