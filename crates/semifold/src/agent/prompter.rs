use anyhow::Context;

use super::client::AgentClient;
use super::config::AgentConfig;
use super::git::ChangesetDiff;
use semifold_resolver::context::Context as ResolverContext;

pub fn generate_changeset(
    config: &AgentConfig,
    diff: &ChangesetDiff,
    ctx: &ResolverContext,
) -> anyhow::Result<String> {
    let prompt = build_prompt(diff, ctx)?;

    let client = AgentClient::new(config.clone());
    let runtime = tokio::runtime::Runtime::new()?;
    let response = runtime.block_on(client.chat(&prompt))?;

    parse_changeset_response(&response, diff, ctx)
}

fn build_prompt(diff: &ChangesetDiff, ctx: &ResolverContext) -> anyhow::Result<String> {
    let config = ctx
        .config
        .as_ref()
        .context("Config not loaded")?;

    let packages_info = config
        .packages
        .iter()
        .map(|(name, pkg)| format!("- {} (path: {}, resolver: {})", name, pkg.path.display(), pkg.resolver))
        .collect::<Vec<_>>()
        .join("\n");

    let tags_info = config
        .tags
        .iter()
        .map(|(key, val)| format!("- {}: {}", key, val))
        .collect::<Vec<_>>()
        .join("\n");

    if diff.is_commit_messages {
        Ok(format!(
            r#"You are a version management assistant. Analyze the following git commit messages and create a changeset for them.

## Available Packages
{packages_info}

## Available Tags
{tags_info}

## Commit Messages
{content}

## Instructions
1. Analyze which packages are likely affected based on the commit messages
2. Determine the appropriate bump level (major/minor/patch) for each package:
   - major: Breaking changes, API changes, or significant refactoring
   - minor: New features or functional improvements
   - patch: Bug fixes, performance improvements, or minor changes
3. Assign appropriate tags if relevant (e.g., feat, fix, chore, refactor, perf)
4. Generate a clear, concise summary describing the core change

## Output Format
Output ONLY the changeset in the following format (no explanations or markdown code blocks):
---
<package-name>: <major|minor|patch>[:<tag>]
<package-name>: <major|minor|patch>[:<tag>]
---

<Summary of changes, concise and descriptive>"#,
            packages_info = packages_info,
            tags_info = tags_info,
            content = diff.diff_content
        ))
    } else {
        Ok(format!(
            r#"You are a version management assistant. Analyze the following git diff and create a changeset for it.

## Available Packages
{packages_info}

## Available Tags
{tags_info}

## Git Diff
{content}

## Instructions
1. Analyze which packages are affected by the changes
2. Determine the appropriate bump level (major/minor/patch) for each affected package based on the diff content:
   - major: Breaking changes, API changes, or significant refactoring
   - minor: New features or functional improvements
   - patch: Bug fixes, performance improvements, or minor changes
3. Assign appropriate tags if relevant (e.g., feat, fix, chore, refactor, perf)
4. Generate a clear, concise summary describing the core change

## Output Format
Output ONLY the changeset in the following format (no explanations or markdown code blocks):
---
<package-name>: <major|minor|patch>[:<tag>]
<package-name>: <major|minor|patch>[:<tag>]
---

<Summary of changes, concise and descriptive>"#,
            packages_info = packages_info,
            tags_info = tags_info,
            content = diff.diff_content
        ))
    }
}

fn parse_changeset_response(
    response: &str,
    diff: &ChangesetDiff,
    ctx: &ResolverContext,
) -> anyhow::Result<String> {
    let content = response.trim();

    let has_front_matter = content.contains("---");

    if has_front_matter {
        let parts: Vec<&str> = content.splitn(2, "---").collect();
        if parts.len() == 2 {
            let after_separator = parts[1];
            if let Some(separator_idx) = after_separator.find("---") {
                let (front_matter, summary) = after_separator.split_at(separator_idx);
                let summary = summary.trim_start_matches("---").trim();

                let changeset = format!("{}\n---\n\n{}\n", front_matter.trim(), summary);
                return Ok(changeset);
            }
        }
    }

    let _config = ctx
        .config
        .as_ref()
        .context("Config not loaded")?;

    let affected_names: Vec<&str> = diff
        .affected_packages
        .iter()
        .map(|p| p.name.as_str())
        .collect();

    if affected_names.is_empty() && !diff.is_commit_messages {
        anyhow::bail!("AI response does not match expected format and no affected packages detected");
    }

    let level = if !diff.affected_packages.is_empty() {
        diff.affected_packages[0].suggested_level.to_string()
    } else {
        "patch".to_string()
    };

    let fallback = format!(
        "\
---
{}
---

{}",
        if affected_names.is_empty() {
            "semifold: patch".to_string()
        } else {
            affected_names
                .iter()
                .map(|name| format!("{}: {}", name, level))
                .collect::<Vec<_>>()
                .join("\n")
        },
        content.lines().take(3).collect::<Vec<_>>().join(" ")
    );

    Ok(fallback)
}
