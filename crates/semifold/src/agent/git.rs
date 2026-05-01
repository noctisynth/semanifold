use inquire::Confirm;

use semifold_resolver::{changeset::BumpLevel, context::Context as ResolverContext};

use super::git_checker::GitChecker;

pub struct ChangesetDiff {
    pub diff_content: String,
    pub affected_packages: Vec<AffectedPackage>,
    pub is_commit_messages: bool,
}

pub struct AffectedPackage {
    pub name: String,
    pub suggested_level: BumpLevel,
}

pub fn get_changeset_diff(ctx: &ResolverContext) -> anyhow::Result<ChangesetDiff> {
    let checker = GitChecker::new();

    let base_branch = ctx
        .config
        .as_ref()
        .map(|c| c.branches.base.clone())
        .unwrap_or_else(|| "main".to_string());

    if let Err(e) = checker.check_has_commits() {
        anyhow::bail!("❌ {}", e);
    }

    if let Err(e) = checker.check_base_branch(&base_branch) {
        anyhow::bail!("❌ {}", e);
    }

    if let Some(msg) = checker.check_dirty() {
        println!("⚠️  {}", msg);
    }

    if let Err(e) = checker.check_new_commits(&base_branch) {
        anyhow::bail!("⚠️  {}", e);
    }

    let diff_content = checker
        .get_diff(&base_branch)
        .map_err(|e| anyhow::anyhow!(e))?;

    if diff_content.is_empty() {
        let use_messages = Confirm::new(
            "No file changes detected, but commits exist.\nDo you want to generate changes from commit messages? (y/n)",
        )
        .with_default(false)
        .prompt()?;

        if use_messages {
            let messages = checker
                .get_commit_messages(&base_branch, 10)
                .map_err(|e| anyhow::anyhow!(e))?;
            return Ok(ChangesetDiff {
                diff_content: messages,
                affected_packages: vec![],
                is_commit_messages: true,
            });
        } else {
            anyhow::bail!("No changes to analyze.");
        }
    }

    let affected_packages = detect_affected_packages(&diff_content, ctx)?;

    Ok(ChangesetDiff {
        diff_content,
        affected_packages,
        is_commit_messages: false,
    })
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
