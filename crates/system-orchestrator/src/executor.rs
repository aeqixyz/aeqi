use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::process::Command;
use tracing::{debug, info, warn};

fn resolve_claude_binary() -> String {
    static CACHED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHED.get_or_init(|| {
        if let Ok(path) = std::process::Command::new("which")
            .arg("claude")
            .output()
        {
            let s = String::from_utf8_lossy(&path.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
        for candidate in [
            dirs::home_dir()
                .map(|h| h.join(".local/bin/claude"))
                .unwrap_or_default(),
            PathBuf::from("/usr/local/bin/claude"),
            PathBuf::from("/usr/bin/claude"),
        ] {
            if candidate.exists() {
                return candidate.to_string_lossy().into_owned();
            }
        }
        "claude".to_string()
    }).clone()
}

const MAX_RETRIES: u32 = 3;
const INITIAL_RETRY_DELAY_MS: u64 = 1000;

/// Result of a Claude Code CLI execution.
#[derive(Debug)]
pub struct ExecutionResult {
    /// The assistant's final response text.
    pub result_text: String,
    /// Session ID (if returned).
    pub session_id: Option<String>,
    /// Number of agentic turns used.
    pub num_turns: u32,
    /// Total cost in USD.
    pub total_cost_usd: f64,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Spirit protocol injected into every Claude Code worker's system prompt.
/// Teaches workers how to report completion, signal blockers, and use sub-agents.
pub const WORKER_PROTOCOL: &str = r#"
## Spirit Protocol

You are a Realm spirit executing a quest. Follow these rules strictly.

### Completion
When you successfully complete the task, provide a clear summary of what you changed.
Include file paths, commit hashes, and any deployment notes.

### Blocked — Need Input
If you cannot complete the task because you need a decision, clarification, or information
that isn't available in the codebase:
- Start your response with exactly: BLOCKED:
- On the next line, state the specific question you need answered
- Then describe what you've done so far and why you're stuck
- Be precise — your question will be passed to another agent or human for resolution

Example:
```
BLOCKED:
Should the new WebSocket endpoint require authentication, or should it be public?
I've implemented the handler and message types in src/ws.rs but need to know
whether to wire it through the auth middleware before proceeding.
```

### Failed — Technical Error
If the task fails due to a build error, test failure, or infrastructure issue you cannot fix:
- Start your response with exactly: FAILED:
- Include the error output and what you tried

### Handoff — Context Exhaustion
If you are running low on context (many tool calls, large codebase exploration) and the task
is not yet complete but you have made meaningful progress:
- Start your response with exactly: HANDOFF:
- Summarize what you have completed so far
- List what remains to be done
- Include any key findings, file paths, or state a successor would need
- A fresh spirit will pick up from your checkpoint

Example:
```
HANDOFF:
Completed: Implemented the debounce buffer in realm-gates/src/telegram.rs (lines 45-120).
Added BufferedMsg struct and timer logic. Tests written in tests/debounce.rs.
Remaining: Wire the buffer into the summoner's message dispatch loop in rm/src/main.rs.
The integration point is around line 1168 where rx.recv() is called.
```

### Sub-Agents
You have full access to Claude Code's Task tool for spawning sub-agents. Use them freely:
- Explore agents for parallel codebase research
- Bash agents for running tests and builds
- general-purpose agents for complex multi-step investigations
Each worker IS an orchestrator — swarm when the task is complex.

### Checkpoints
Previous attempts on this quest (if any) are listed under the Previous Attempts section.
Review them before starting. Don't redo work that's already complete.
If you see file paths, commits, or branches from previous attempts, verify their
current state before building on them.

### Git Workflow
Follow the project's CLAUDE.md for git workflow (worktrees, branches, commits).
"#;

/// Spawns Claude Code CLI instances for bead execution.
///
/// Each execution is ephemeral: no session persistence, no interactive mode.
/// The worker's identity is injected via `--append-system-prompt` and the
/// repo's CLAUDE.md is auto-discovered from the working directory.
///
/// NO tool restrictions — workers get full Claude Code access including
/// Edit, Grep, Glob, Task (sub-agents), Bash, Read, Write, and everything else.
pub struct ClaudeCodeExecutor {
    /// Working directory (rig's repo path).
    workdir: PathBuf,
    /// Claude Code model (e.g., "claude-sonnet-4-6").
    model: String,
    /// Max agentic turns per execution.
    max_turns: u32,
    /// Max budget in USD per execution (None = unlimited).
    max_budget_usd: Option<f64>,
}

impl ClaudeCodeExecutor {
    pub fn new(
        workdir: PathBuf,
        model: String,
        max_turns: u32,
        max_budget_usd: Option<f64>,
    ) -> Self {
        Self {
            workdir,
            model,
            max_turns,
            max_budget_usd,
        }
    }

    /// Get the working directory for this executor.
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// Execute a bead via Claude Code CLI with retry on transient failures.
    ///
    /// Spawns `claude -p "<quest_context>"` with the rig's identity + worker protocol
    /// as `--append-system-prompt`. Retries up to MAX_RETRIES times with exponential
    /// backoff on spawn failures or non-zero exit codes.
    pub async fn execute(
        &self,
        identity: &system_core::Identity,
        quest_context: &str,
    ) -> Result<ExecutionResult> {
        let system_prompt = {
            let budget = crate::context_budget::ContextBudget::default();
            let mut sp = budget.apply_to_identity(identity);
            sp.push_str("\n\n---\n\n");
            sp.push_str(WORKER_PROTOCOL);
            sp
        };

        let mut last_err = None;

        for attempt in 1..=MAX_RETRIES {
            match self.execute_once(&system_prompt, quest_context).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let is_parse_error = e.to_string().contains("failed to parse claude code JSON");
                    if is_parse_error || attempt == MAX_RETRIES {
                        return Err(e);
                    }
                    let delay = INITIAL_RETRY_DELAY_MS * 2u64.pow(attempt - 1);
                    warn!(
                        attempt,
                        max = MAX_RETRIES,
                        delay_ms = delay,
                        error = %e,
                        "transient failure, retrying"
                    );
                    last_err = Some(e);
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("all retries exhausted")))
    }

    /// Single execution attempt (no retry).
    async fn execute_once(
        &self,
        system_prompt: &str,
        quest_context: &str,
    ) -> Result<ExecutionResult> {
        let start = Instant::now();

        let claude_bin = resolve_claude_binary();
        let mut cmd = Command::new(&claude_bin);

        cmd.arg("-p").arg(quest_context);
        cmd.arg("--output-format").arg("json");
        cmd.arg("--permission-mode").arg("bypassPermissions");
        cmd.arg("--model").arg(&self.model);
        cmd.arg("--max-turns").arg(self.max_turns.to_string());
        cmd.arg("--no-session-persistence");

        if let Some(budget) = self.max_budget_usd {
            cmd.arg("--max-budget-usd").arg(budget.to_string());
        }

        cmd.arg("--append-system-prompt").arg(system_prompt);
        cmd.current_dir(&self.workdir);
        cmd.env_remove("CLAUDECODE");
        cmd.env_remove("CLAUDE_CODE");

        debug!(
            workdir = %self.workdir.display(),
            model = %self.model,
            max_turns = self.max_turns,
            "spawning claude code (unrestricted)"
        );

        let timeout_secs = (self.max_turns as u64 * 300).max(1800);
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            cmd.output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!(
            "claude code timed out after {}s (max_turns={})",
            timeout_secs, self.max_turns,
        ))?
        .context("failed to spawn claude CLI — is it installed?")?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            warn!(
                exit_code = ?output.status.code(),
                stderr = %stderr,
                "claude code exited with error"
            );
            anyhow::bail!(
                "claude code failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                if stderr.is_empty() { &stdout } else { &stderr },
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_json_output(&stdout, duration_ms)
    }

    /// Parse the `--output-format json` response from Claude Code.
    fn parse_json_output(stdout: &str, duration_ms: u64) -> Result<ExecutionResult> {
        let v: serde_json::Value = serde_json::from_str(stdout)
            .context("failed to parse claude code JSON output")?;

        let result_text = v.get("result")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string();

        let session_id = v.get("session_id")
            .and_then(|s| s.as_str())
            .map(String::from);

        let num_turns = v.get("num_turns")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32;

        let cost_missing = v.get("total_cost_usd").is_none();
        let total_cost_usd = v.get("total_cost_usd")
            .and_then(|c| c.as_f64())
            .unwrap_or(0.0);

        if cost_missing {
            warn!("claude code response missing cost data — actual cost unknown");
        }

        info!(
            turns = num_turns,
            cost_usd = total_cost_usd,
            duration_ms = duration_ms,
            result_len = result_text.len(),
            "claude code execution complete"
        );

        Ok(ExecutionResult {
            result_text,
            session_id,
            num_turns,
            total_cost_usd,
            duration_ms,
        })
    }
}

/// Parsed outcome from a worker's result text.
#[derive(Debug, Clone)]
pub enum TaskOutcome {
    /// Task completed successfully.
    Done(String),
    /// Spirit is blocked and needs input to continue.
    Blocked {
        /// The specific question or information needed.
        question: String,
        /// Full result text including work done so far.
        full_text: String,
    },
    /// Spirit hit context exhaustion but made progress. Re-queue with checkpoint.
    Handoff {
        /// Summary of progress made and what remains.
        checkpoint: String,
    },
    /// Task failed due to a technical error.
    Failed(String),
}

impl TaskOutcome {
    /// Parse a worker's result text into a structured outcome.
    ///
    /// Checks the first non-empty line for BLOCKED: or FAILED: prefix.
    /// This prevents false positives from code blocks that happen to
    /// contain these words in the middle of output.
    pub fn parse(result_text: &str) -> Self {
        let trimmed = result_text.trim();

        // Get the first non-empty line to check for outcome prefix.
        let first_line = trimmed
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();

        if first_line.starts_with("BLOCKED:") {
            // Extract everything after the first-line prefix as the blocker text.
            let after_prefix = if first_line == "BLOCKED:" {
                // Prefix alone on first line — question is on subsequent lines.
                trimmed
                    .strip_prefix("BLOCKED:")
                    .unwrap_or(trimmed)
                    .trim()
            } else {
                // Prefix + text on same line.
                first_line
                    .strip_prefix("BLOCKED:")
                    .unwrap_or("")
                    .trim()
            };
            let question = after_prefix
                .split("\n\n")
                .next()
                .unwrap_or(after_prefix)
                .trim()
                .to_string();
            Self::Blocked {
                question,
                full_text: result_text.to_string(),
            }
        } else if first_line.starts_with("HANDOFF:") {
            let checkpoint = trimmed
                .strip_prefix("HANDOFF:")
                .unwrap_or(trimmed)
                .trim()
                .to_string();
            Self::Handoff { checkpoint }
        } else if first_line.starts_with("FAILED:") {
            Self::Failed(result_text.to_string())
        } else {
            Self::Done(result_text.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_output() {
        let json = r#"{
            "type": "result",
            "result": "I fixed the bug in main.rs",
            "session_id": "abc-123",
            "num_turns": 3,
            "total_cost_usd": 0.08
        }"#;

        let result = ClaudeCodeExecutor::parse_json_output(json, 5000).unwrap();
        assert_eq!(result.result_text, "I fixed the bug in main.rs");
        assert_eq!(result.session_id, Some("abc-123".to_string()));
        assert_eq!(result.num_turns, 3);
        assert!((result.total_cost_usd - 0.08).abs() < f64::EPSILON);
        assert_eq!(result.duration_ms, 5000);
    }

    #[test]
    fn test_parse_minimal_json() {
        let json = r#"{"type": "result", "result": "done"}"#;
        let result = ClaudeCodeExecutor::parse_json_output(json, 100).unwrap();
        assert_eq!(result.result_text, "done");
        assert_eq!(result.num_turns, 0);
        assert_eq!(result.total_cost_usd, 0.0);
    }

    #[test]
    fn test_worker_outcome_done() {
        let outcome = TaskOutcome::parse("I fixed the bug and committed to feat/fix-pms.");
        assert!(matches!(outcome, TaskOutcome::Done(_)));
    }

    #[test]
    fn test_worker_outcome_blocked() {
        let text = "BLOCKED:\nShould auth be JWT or session-based?\n\nI've set up the middleware but need to know the auth strategy.";
        let outcome = TaskOutcome::parse(text);
        match outcome {
            TaskOutcome::Blocked { question, .. } => {
                assert_eq!(question, "Should auth be JWT or session-based?");
            }
            _ => panic!("expected Blocked"),
        }
    }

    #[test]
    fn test_worker_outcome_failed() {
        let outcome = TaskOutcome::parse("FAILED:\ncargo build returned 3 errors in pms/src/main.rs");
        assert!(matches!(outcome, TaskOutcome::Failed(_)));
    }

    #[test]
    fn test_worker_outcome_handoff() {
        let text = "HANDOFF:\nCompleted: Implemented debounce buffer.\nRemaining: Wire into summoner dispatch loop.";
        let outcome = TaskOutcome::parse(text);
        match outcome {
            TaskOutcome::Handoff { checkpoint } => {
                assert!(checkpoint.contains("Completed: Implemented debounce buffer"));
                assert!(checkpoint.contains("Remaining:"));
            }
            _ => panic!("expected Handoff"),
        }
    }
}
