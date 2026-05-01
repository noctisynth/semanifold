use std::process::Command;

pub struct GitChecker;

impl GitChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn check_has_commits(&self) -> Result<(), String> {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output();

        match output {
            Ok(out) if out.status.success() => Ok(()),
            _ => Err("Repository has no commits yet.".to_string()),
        }
    }

    pub fn check_base_branch(&self, base: &str) -> Result<(), String> {
        let output = Command::new("git")
            .args(["rev-parse", base])
            .output();

        match output {
            Ok(out) if out.status.success() => Ok(()),
            _ => Err(format!(
                "Base branch '{}' not found. Please fetch or configure a valid base branch.",
                base
            )),
        }
    }

    pub fn check_dirty(&self) -> Option<String> {
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let status = String::from_utf8_lossy(&out.stdout);
                if status.trim().is_empty() {
                    None
                } else {
                    Some("Uncommitted changes detected.".to_string())
                }
            }
            _ => None,
        }
    }

    pub fn check_new_commits(&self, base: &str) -> Result<(), String> {
        let output = Command::new("git")
            .args(["rev-list", "--count", &format!("{}..HEAD", base)])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let count = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if count == "0" {
                    Err("No new commits found compared to base branch.".to_string())
                } else {
                    Ok(())
                }
            }
            Err(e) => Err(format!("Failed to check commits: {}", e)),
            _ => Err("Failed to check commits.".to_string()),
        }
    }

    pub fn get_diff(&self, base: &str) -> Result<String, String> {
        let output = Command::new("git")
            .args(["diff", &format!("{}...HEAD", base)])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                Ok(String::from_utf8_lossy(&out.stdout).to_string())
            }
            Err(e) => Err(format!("Failed to get diff: {}", e)),
            _ => Err("Failed to get diff.".to_string()),
        }
    }

    pub fn get_commit_messages(&self, base: &str, count: usize) -> Result<String, String> {
        let output = Command::new("git")
            .args([
                "log",
                &format!("{}..HEAD", base),
                "--pretty=format:%s",
                &format!("-{}", count.to_string()),
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                Ok(String::from_utf8_lossy(&out.stdout).to_string())
            }
            Err(e) => Err(format!("Failed to get commit messages: {}", e)),
            _ => Err("Failed to get commit messages.".to_string()),
        }
    }
}

impl Default for GitChecker {
    fn default() -> Self {
        Self::new()
    }
}
