use anyhow::Result;
use async_trait::async_trait;
use system_core::traits::{ToolResult, ToolSpec};
use system_core::traits::Tool;
use std::path::PathBuf;
use std::process::Command;

/// Tool for git worktree management.
pub struct GitWorktreeTool {
    repo_root: PathBuf,
    worktree_root: PathBuf,
}

impl GitWorktreeTool {
    pub fn new(repo_root: PathBuf, worktree_root: PathBuf) -> Self {
        Self { repo_root, worktree_root }
    }

    fn run_git(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_root)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(stdout)
        } else {
            anyhow::bail!("{stderr}")
        }
    }
}

#[async_trait]
impl Tool for GitWorktreeTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let action = args.get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        match action {
            "create" => {
                let branch = args.get("branch")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing branch name"))?;

                let worktree_path = self.worktree_root.join(branch);
                let path_str = worktree_path.to_string_lossy();

                let result = self.run_git(&["worktree", "add", &path_str, "-b", branch]);
                match result {
                    Ok(output) => Ok(ToolResult::success(format!("Created worktree at {path_str}\n{output}"))),
                    Err(e) => Ok(ToolResult::error(format!("Failed to create worktree: {e}"))),
                }
            }

            "remove" => {
                let branch = args.get("branch")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing branch name"))?;

                let worktree_path = self.worktree_root.join(branch);
                let path_str = worktree_path.to_string_lossy();

                let result = self.run_git(&["worktree", "remove", &path_str, "--force"]);
                match result {
                    Ok(_) => {
                        // Also delete the branch.
                        let _ = self.run_git(&["branch", "-d", branch]);
                        Ok(ToolResult::success(format!("Removed worktree and branch: {branch}")))
                    }
                    Err(e) => Ok(ToolResult::error(format!("Failed to remove worktree: {e}"))),
                }
            }

            "list" => {
                match self.run_git(&["worktree", "list", "--porcelain"]) {
                    Ok(output) => Ok(ToolResult::success(output)),
                    Err(e) => Ok(ToolResult::error(format!("Failed to list worktrees: {e}"))),
                }
            }

            "status" => {
                let branch = args.get("branch")
                    .and_then(|v| v.as_str());

                let dir = if let Some(b) = branch {
                    self.worktree_root.join(b)
                } else {
                    self.repo_root.clone()
                };

                let output = Command::new("git")
                    .args(["status", "--short"])
                    .current_dir(&dir)
                    .output()?;

                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                if stdout.is_empty() {
                    Ok(ToolResult::success("Working tree clean."))
                } else {
                    Ok(ToolResult::success(stdout))
                }
            }

            "merge" => {
                let branch = args.get("branch")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing branch name"))?;
                let target = args.get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("dev");

                // Checkout target, merge branch.
                self.run_git(&["checkout", target])?;
                match self.run_git(&["merge", branch]) {
                    Ok(output) => Ok(ToolResult::success(format!("Merged {branch} into {target}\n{output}"))),
                    Err(e) => {
                        // Try to recover.
                        let _ = self.run_git(&["merge", "--abort"]);
                        Ok(ToolResult::error(format!("Merge failed: {e}")))
                    }
                }
            }

            other => Ok(ToolResult::error(format!("Unknown action: {other}. Use: create, remove, list, status, merge"))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "git_worktree".to_string(),
            description: "Manage git worktrees: create, remove, list, status, or merge branches.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["create", "remove", "list", "status", "merge"],
                        "description": "Action to perform"
                    },
                    "branch": { "type": "string", "description": "Branch name (required for create/remove/merge)" },
                    "target": { "type": "string", "description": "Target branch for merge (default: dev)", "default": "dev" }
                },
                "required": ["action"]
            }),
        }
    }

    fn name(&self) -> &str { "git_worktree" }
}
