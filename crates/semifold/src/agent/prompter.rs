use anyhow::Context;
use inquire::Select;

use super::client::AgentClient;
use super::config::AgentConfig;
use super::git::ChangesetDiff;
use semifold_resolver::changeset::Changeset;
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

fn handle_ai_error(response: &str, diff: &ChangesetDiff) -> anyhow::Result<String> {
    println!("AI returned invalid changeset format.");
    println!(
        "Raw response (first 200 chars): {}",
        &response[..response.len().min(200)]
    );

    let options = vec!["Regenerate", "Manual edit", "Exit"];
    let choice = Select::new("What would you like to do?", options).prompt()?;

    match choice {
        "Regenerate" => anyhow::bail!("regenerate"),
        "Manual edit" => {
            let summary = response.lines().take(3).collect::<Vec<_>>().join(" ");
            let changeset = format!(
                "---\n{}\n---\n\n{}",
                if diff.affected_packages.is_empty() {
                    "semifold: patch".to_string()
                } else {
                    diff.affected_packages
                        .iter()
                        .map(|p| format!("{}: {}", p.name, p.suggested_level))
                        .collect::<Vec<_>>()
                        .join("\n")
                },
                summary
            );
            Ok(changeset)
        }
        _ => anyhow::bail!("User exited"),
    }
}

fn build_prompt(diff: &ChangesetDiff, ctx: &ResolverContext) -> anyhow::Result<String> {
    let config = ctx.config.as_ref().context("Config not loaded")?;

    let packages_info = config
        .packages
        .iter()
        .map(|(name, pkg)| {
            format!(
                "- {} (path: {}, resolver: {})",
                name,
                pkg.path.display(),
                pkg.resolver
            )
        })
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

<Summary of changes, concise and descriptive>

## Example
---
semifold: minor:feat
docs: patch:fix
---

Add AI-powered changeset generation with configurable provider support"#,
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

<Summary of changes, concise and descriptive>

## Example
---
semifold: minor:feat
docs: patch:fix
---

Add AI-powered changeset generation with configurable provider support"#,
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

                let temp_path = std::env::temp_dir().join("test_changeset.md");
                if std::fs::write(&temp_path, &changeset).is_ok() {
                    if Changeset::from_file(ctx, &temp_path).is_ok() {
                        std::fs::remove_file(&temp_path).ok();
                        return Ok(changeset);
                    }
                    std::fs::remove_file(&temp_path).ok();
                }

                return Ok(changeset);
            }
        }
    }

    handle_ai_error(content, diff)
}
