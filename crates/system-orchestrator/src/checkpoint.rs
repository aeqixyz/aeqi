use anyhow::{Context, Result};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// External checkpoint for a worker's work-in-progress.
///
/// Inspired by Gastown's GUPP pattern: checkpoints are captured **externally** by
/// inspecting git state, NOT self-reported by the agent. Agents are unreliable
/// reporters — git is the source of truth.
///
/// The checkpoint records observable state (modified files, last commit, branch) that
/// a successor spirit can use to understand what was done and resume work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCheckpoint {
    /// Task ID this checkpoint is associated with.
    pub task_id: Option<String>,
    /// Name of the spirit that was working.
    pub worker_name: Option<String>,
    /// Files modified according to `git status --porcelain`.
    pub modified_files: Vec<String>,
    /// Last commit hash from `git rev-parse HEAD`.
    pub last_commit: Option<String>,
    /// Current branch from `git rev-parse --abbrev-ref HEAD`.
    pub branch: Option<String>,
    /// Working directory path the spirit was operating in.
    pub worktree_path: Option<String>,
    /// When this checkpoint was captured.
    pub timestamp: DateTime<Utc>,
    /// Session ID (if the spirit had one).
    pub session_id: Option<String>,
    /// Free-form progress notes (e.g., from the spirit's last output).
    pub progress_notes: Option<String>,
}

impl AgentCheckpoint {
    /// Capture checkpoint by inspecting git state externally.
    ///
    /// Runs git commands against the spirit's working directory to observe:
    /// - Modified/staged/untracked files (`git status --porcelain`)
    /// - Last commit hash (`git rev-parse HEAD`)
    /// - Current branch (`git rev-parse --abbrev-ref HEAD`)
    ///
    /// This is the GUPP insight: observe the agent's work externally via git,
    /// rather than trusting the agent's self-report.
    pub fn capture(workdir: &Path) -> Result<Self> {
        let modified_files = Self::git_modified_files(workdir)?;
        let last_commit = Self::git_last_commit(workdir).ok();
        let branch = Self::git_branch(workdir).ok();

        Ok(Self {
            task_id: None,
            worker_name: None,
            modified_files,
            last_commit,
            branch,
            worktree_path: Some(workdir.to_string_lossy().into_owned()),
            timestamp: Utc::now(),
            session_id: None,
            progress_notes: None,
        })
    }

    /// Set the quest ID on this checkpoint.
    pub fn with_quest_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    /// Set the spirit name on this checkpoint.
    pub fn with_worker_name(mut self, name: impl Into<String>) -> Self {
        self.worker_name = Some(name.into());
        self
    }

    /// Set progress notes on this checkpoint.
    pub fn with_progress_notes(mut self, notes: impl Into<String>) -> Self {
        self.progress_notes = Some(notes.into());
        self
    }

    /// Write checkpoint atomically (temp file + rename).
    ///
    /// Uses write-to-temp + rename pattern to avoid partial writes on crash.
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create checkpoint dir: {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(self)
            .context("failed to serialize checkpoint")?;

        // Write to temp file first, then rename for atomicity.
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, &json)
            .with_context(|| format!("failed to write temp checkpoint: {}", tmp_path.display()))?;

        std::fs::rename(&tmp_path, path)
            .with_context(|| format!("failed to rename checkpoint: {} -> {}", tmp_path.display(), path.display()))?;

        debug!(path = %path.display(), "checkpoint written");
        Ok(())
    }

    /// Read checkpoint from file. Returns Ok(None) if no checkpoint file exists.
    pub fn read(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }

        let json = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read checkpoint: {}", path.display()))?;

        let checkpoint: Self = serde_json::from_str(&json)
            .with_context(|| format!("failed to parse checkpoint: {}", path.display()))?;

        debug!(path = %path.display(), task_id = ?checkpoint.task_id, "checkpoint loaded");
        Ok(Some(checkpoint))
    }

    /// Check if this checkpoint is stale (older than the given threshold).
    pub fn is_stale(&self, max_age: TimeDelta) -> bool {
        Utc::now() - self.timestamp > max_age
    }

    /// Remove checkpoint file. No error if file does not exist.
    pub fn remove(path: &Path) -> Result<()> {
        if path.exists() {
            std::fs::remove_file(path)
                .with_context(|| format!("failed to remove checkpoint: {}", path.display()))?;
            debug!(path = %path.display(), "checkpoint removed");
        }
        Ok(())
    }

    /// Standard checkpoint file path for a given quest in a project's .sigil directory.
    ///
    /// Layout: `<project_dir>/.sigil/checkpoints/<task_id>.json`
    pub fn path_for_quest(project_dir: &Path, task_id: &str) -> PathBuf {
        project_dir
            .join(".sigil")
            .join("checkpoints")
            .join(format!("{task_id}.json"))
    }

    /// Format this checkpoint as context for injection into a successor spirit's prompt.
    ///
    /// Produces a human-readable summary that a new spirit can use to understand
    /// what the previous spirit observed.
    pub fn as_context(&self) -> String {
        let mut ctx = String::from("## External Checkpoint (git state capture)\n\n");

        if let Some(ref branch) = self.branch {
            ctx.push_str(&format!("**Branch:** `{branch}`\n"));
        }

        if let Some(ref commit) = self.last_commit {
            ctx.push_str(&format!("**Last commit:** `{commit}`\n"));
        }

        ctx.push_str(&format!("**Captured at:** {}\n", self.timestamp));

        if !self.modified_files.is_empty() {
            ctx.push_str(&format!(
                "\n**Modified files ({}):**\n",
                self.modified_files.len()
            ));
            for f in &self.modified_files {
                ctx.push_str(&format!("- `{f}`\n"));
            }
        } else {
            ctx.push_str("\n**No modified files detected.**\n");
        }

        if let Some(ref notes) = self.progress_notes {
            ctx.push_str(&format!("\n**Spirit's last notes:**\n{notes}\n"));
        }

        ctx.push_str(
            "\nVerify the current state of these files before building on them. \
             The previous spirit may have been interrupted.\n",
        );

        ctx
    }

    // --- Git helper commands ---

    /// Get modified files from `git status --porcelain`.
    fn git_modified_files(workdir: &Path) -> Result<Vec<String>> {
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(workdir)
            .output()
            .context("failed to run git status")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(workdir = %workdir.display(), stderr = %stderr, "git status failed");
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let files: Vec<String> = stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                // Porcelain format: "XY filename" — skip the 2-char status + space.
                if l.len() > 3 {
                    l[3..].to_string()
                } else {
                    l.to_string()
                }
            })
            .collect();

        Ok(files)
    }

    /// Get last commit hash from `git rev-parse HEAD`.
    fn git_last_commit(workdir: &Path) -> Result<String> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(workdir)
            .output()
            .context("failed to run git rev-parse HEAD")?;

        if !output.status.success() {
            anyhow::bail!("git rev-parse HEAD failed");
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get current branch from `git rev-parse --abbrev-ref HEAD`.
    fn git_branch(workdir: &Path) -> Result<String> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(workdir)
            .output()
            .context("failed to run git rev-parse --abbrev-ref HEAD")?;

        if !output.status.success() {
            anyhow::bail!("git rev-parse --abbrev-ref HEAD failed");
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_checkpoint_write_and_read() {
        let dir = TempDir::new().unwrap();
        let cp_path = dir.path().join("test-checkpoint.json");

        let checkpoint = AgentCheckpoint {
            task_id: Some("sg-042".to_string()),
            worker_name: Some("sigil-worker-1".to_string()),
            modified_files: vec![
                "src/main.rs".to_string(),
                "Cargo.toml".to_string(),
            ],
            last_commit: Some("abc123def456".to_string()),
            branch: Some("feat/gupp-checkpoint".to_string()),
            worktree_path: Some("/home/dev/worktrees/feat/test".to_string()),
            timestamp: Utc::now(),
            session_id: None,
            progress_notes: Some("Implemented the widget parser".to_string()),
        };

        checkpoint.write(&cp_path).unwrap();
        assert!(cp_path.exists());

        let loaded = AgentCheckpoint::read(&cp_path).unwrap().unwrap();
        assert_eq!(loaded.task_id, Some("sg-042".to_string()));
        assert_eq!(loaded.worker_name, Some("sigil-worker-1".to_string()));
        assert_eq!(loaded.modified_files.len(), 2);
        assert_eq!(loaded.last_commit, Some("abc123def456".to_string()));
        assert_eq!(loaded.branch, Some("feat/gupp-checkpoint".to_string()));
        assert_eq!(
            loaded.progress_notes,
            Some("Implemented the widget parser".to_string())
        );
    }

    #[test]
    fn test_checkpoint_read_missing_file() {
        let dir = TempDir::new().unwrap();
        let cp_path = dir.path().join("nonexistent.json");

        let result = AgentCheckpoint::read(&cp_path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_checkpoint_remove() {
        let dir = TempDir::new().unwrap();
        let cp_path = dir.path().join("to-remove.json");

        let checkpoint = AgentCheckpoint {
            task_id: None,
            worker_name: None,
            modified_files: vec![],
            last_commit: None,
            branch: None,
            worktree_path: None,
            timestamp: Utc::now(),
            session_id: None,
            progress_notes: None,
        };
        checkpoint.write(&cp_path).unwrap();
        assert!(cp_path.exists());

        AgentCheckpoint::remove(&cp_path).unwrap();
        assert!(!cp_path.exists());

        // Removing again should not error.
        AgentCheckpoint::remove(&cp_path).unwrap();
    }

    #[test]
    fn test_checkpoint_is_stale() {
        let recent = AgentCheckpoint {
            task_id: None,
            worker_name: None,
            modified_files: vec![],
            last_commit: None,
            branch: None,
            worktree_path: None,
            timestamp: Utc::now(),
            session_id: None,
            progress_notes: None,
        };

        assert!(!recent.is_stale(TimeDelta::hours(1)));

        let old = AgentCheckpoint {
            timestamp: Utc::now() - TimeDelta::hours(2),
            ..recent.clone()
        };
        assert!(old.is_stale(TimeDelta::hours(1)));
        assert!(!old.is_stale(TimeDelta::hours(3)));
    }

    #[test]
    fn test_checkpoint_path_for_quest() {
        let path = AgentCheckpoint::path_for_quest(Path::new("/home/dev/projects/gacha-agency"), "sg-001");
        assert_eq!(
            path,
            PathBuf::from("/home/dev/projects/gacha-agency/.sigil/checkpoints/sg-001.json")
        );
    }

    #[test]
    fn test_checkpoint_as_context() {
        let checkpoint = AgentCheckpoint {
            task_id: Some("sg-042".to_string()),
            worker_name: Some("sigil-worker-1".to_string()),
            modified_files: vec![
                "src/main.rs".to_string(),
                "Cargo.toml".to_string(),
            ],
            last_commit: Some("abc123def456".to_string()),
            branch: Some("feat/gupp-checkpoint".to_string()),
            worktree_path: Some("/home/dev/worktrees/feat/test".to_string()),
            timestamp: Utc::now(),
            session_id: None,
            progress_notes: Some("Implemented the widget parser".to_string()),
        };

        let ctx = checkpoint.as_context();
        assert!(ctx.contains("External Checkpoint"));
        assert!(ctx.contains("feat/gupp-checkpoint"));
        assert!(ctx.contains("abc123def456"));
        assert!(ctx.contains("src/main.rs"));
        assert!(ctx.contains("Cargo.toml"));
        assert!(ctx.contains("Implemented the widget parser"));
        assert!(ctx.contains("Modified files (2)"));
    }

    #[test]
    fn test_checkpoint_as_context_empty() {
        let checkpoint = AgentCheckpoint {
            task_id: None,
            worker_name: None,
            modified_files: vec![],
            last_commit: None,
            branch: None,
            worktree_path: None,
            timestamp: Utc::now(),
            session_id: None,
            progress_notes: None,
        };

        let ctx = checkpoint.as_context();
        assert!(ctx.contains("No modified files detected"));
    }

    #[test]
    fn test_checkpoint_builder_methods() {
        let checkpoint = AgentCheckpoint {
            task_id: None,
            worker_name: None,
            modified_files: vec![],
            last_commit: None,
            branch: None,
            worktree_path: None,
            timestamp: Utc::now(),
            session_id: None,
            progress_notes: None,
        };

        let cp = checkpoint
            .with_quest_id("sg-001")
            .with_worker_name("sigil-worker-1")
            .with_progress_notes("Did some work");

        assert_eq!(cp.task_id, Some("sg-001".to_string()));
        assert_eq!(cp.worker_name, Some("sigil-worker-1".to_string()));
        assert_eq!(cp.progress_notes, Some("Did some work".to_string()));
    }

    #[test]
    fn test_capture_in_git_repo() {
        // Create a temp git repo to test capture against.
        let dir = TempDir::new().unwrap();
        let workdir = dir.path();

        // Initialize a git repo.
        let init = std::process::Command::new("git")
            .args(["init"])
            .current_dir(workdir)
            .output();

        if init.is_err() || !init.unwrap().status.success() {
            // Git not available in test env — skip gracefully.
            return;
        }

        // Configure git user for the test repo.
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(workdir)
            .output();
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(workdir)
            .output();

        // Create a file and commit it.
        std::fs::write(workdir.join("hello.txt"), "hello").unwrap();
        let _ = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(workdir)
            .output();
        let _ = std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(workdir)
            .output();

        // Modify a file so git status shows changes.
        std::fs::write(workdir.join("hello.txt"), "hello world").unwrap();

        let checkpoint = AgentCheckpoint::capture(workdir).unwrap();
        assert!(checkpoint.last_commit.is_some());
        assert!(!checkpoint.last_commit.as_ref().unwrap().is_empty());
        assert!(checkpoint.branch.is_some());
        assert!(!checkpoint.modified_files.is_empty());
        assert!(checkpoint.modified_files.iter().any(|f| f.contains("hello.txt")));
    }
}
