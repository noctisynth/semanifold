use anyhow::Context;
use git2::{Diff, DiffOptions, Repository};

use semifold_resolver::{changeset::BumpLevel, context::Context as ResolverContext};

pub struct ChangesetDiff {
    pub diff_content: String,
    pub affected_packages: Vec<AffectedPackage>,
}

pub struct AffectedPackage {
    pub name: String,
    pub suggested_level: BumpLevel,
}

pub fn get_changeset_diff(ctx: &ResolverContext) -> anyhow::Result<ChangesetDiff> {
    let repo = Repository::open(".").context("Failed to open git repository")?;

    let head = repo.head().context("Failed to get HEAD")?;
    let head_commit = head.peel_to_commit().context("Failed to peel to commit")?;

    let base_branch = ctx
        .config
        .as_ref()
        .map(|c| c.branches.base.clone())
        .unwrap_or_else(|| "main".to_string());

    let base_commit = repo
        .resolve_reference_from_short_name(&base_branch)?
        .peel_to_commit()
        .context("Failed to get base commit")?;

    let mut diff_opts = DiffOptions::new();
    diff_opts.context_lines(3);

    let diff = repo
        .diff_tree_to_tree(
            Some(&base_commit.tree().context("Failed to get base tree")?),
            Some(&head_commit.tree().context("Failed to get head tree")?),
            Some(&mut diff_opts),
        )
        .context("Failed to get diff")?;

    let diff_content = format_diff(&diff)?;

    let affected_packages = detect_affected_packages(&diff_content, ctx)?;

    Ok(ChangesetDiff {
        diff_content,
        affected_packages,
    })
}

fn format_diff(diff: &Diff) -> anyhow::Result<String> {
    let mut output = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let prefix = match line.origin() {
            '+' => "+",
            '-' => "-",
            ' ' => " ",
            _ => return true,
        };
        output.push_str(prefix);
        output.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
        true
    })
    .context("Failed to print diff")?;
    Ok(output)
}

fn detect_affected_packages(
    diff_content: &str,
    ctx: &ResolverContext,
) -> anyhow::Result<Vec<AffectedPackage>> {
    let config = ctx
        .config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Config not loaded"))?;

    let mut affected = Vec::new();

    for (package_name, package_config) in &config.packages {
        let package_path_str = package_config.path.to_string_lossy();

        if diff_content.contains(&*package_path_str) {
            let suggested_level = infer_bump_level(diff_content);

            affected.push(AffectedPackage {
                name: package_name.clone(),
                suggested_level,
            });
        }
    }

    Ok(affected)
}

fn infer_bump_level(diff_content: &str) -> BumpLevel {
    let diff_lower = diff_content.to_lowercase();

    if diff_lower.contains("breaking")
        || diff_lower.contains("major")
        || diff_lower.contains("breaking change")
    {
        BumpLevel::Major
    } else if diff_lower.contains("feat") || diff_lower.contains("feature") {
        BumpLevel::Minor
    } else {
        BumpLevel::Patch
    }
}
