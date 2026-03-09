use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

fn resolve_claude_binary() -> String {
    static CACHED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHED
        .get_or_init(|| {
            if let Ok(path) = std::process::Command::new("which").arg("claude").output() {
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
        })
        .clone()
}

const INITIAL_RETRY_DELAY_MS: u64 = 1000;

/// Callback invoked with the latest known cost during execution.
/// The current Claude stream surfaces cost on the final result event only.
/// Return `false` to abort once that cost is known.
pub type CostCallback = Arc<dyn Fn(f64) -> bool + Send + Sync>;

/// Real-time progress from a streaming execution.
#[derive(Debug, Clone, Default)]
pub struct ExecutionProgress {
    pub turns_so_far: u32,
    pub cost_so_far: f64,
    pub last_tool: Option<String>,
}

/// Parsed stream event from `--output-format stream-json`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum StreamEvent {
    #[serde(rename = "system")]
    System {
        #[serde(default)]
        subtype: String,
    },
    #[serde(rename = "assistant")]
    Assistant {
        #[serde(default)]
        message: AssistantMessage,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        #[serde(default)]
        name: String,
    },
    #[serde(rename = "tool_result")]
    ToolResult {},
    #[serde(rename = "result")]
    Result {
        #[serde(default)]
        result: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        num_turns: u32,
        #[serde(default)]
        total_cost_usd: f64,
        #[serde(default)]
        duration_ms: u64,
    },
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct AssistantMessage {
    #[serde(default)]
    content: Option<String>,
}

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

/// Worker protocol injected into every Claude Code worker's system prompt.
/// Teaches workers how to report completion, signal blockers, and use sub-agents.
pub const WORKER_PROTOCOL: &str = r#"
## Worker Protocol

You are a worker executing a task. Follow these rules strictly.

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
- A fresh worker will pick up from your checkpoint

Example:
```
HANDOFF:
Completed: Implemented the debounce buffer in sigil-gates/src/telegram.rs (lines 45-120).
Added BufferedMsg struct and timer logic. Tests written in tests/debounce.rs.
Remaining: Wire the buffer into the daemon's message dispatch loop in rm/src/main.rs.
The integration point is around line 1168 where rx.recv() is called.
```

### Context
All project context is provided in your system prompt. Do not search for CLAUDE.md.
Project skills are available at the skills/ directory referenced in your operating instructions.
Read relevant skills before starting specialized work.

### Sub-Agents
You have full access to Claude Code's Task tool for spawning sub-agents. Use them freely:
- Explore agents for parallel codebase research
- Bash agents for running tests and builds
- general-purpose agents for complex multi-step investigations
Each worker IS an orchestrator — swarm when the task is complex.
Subagent specs are at the subagents/ directory referenced in your operating instructions.

### Checkpoints
Previous attempts on this task (if any) are listed under the Previous Attempts section.
Review them before starting. Don't redo work that's already complete.
If you see file paths, commits, or branches from previous attempts, verify their
current state before building on them.

### Git Workflow
Follow the git workflow specified in your operating instructions (worktrees, branches, commits).
"#;

/// Spawns Claude Code CLI instances for task execution.
///
/// Each execution is ephemeral: no session persistence, no interactive mode.
/// The worker's identity is injected via `--append-system-prompt` from the
/// sigil identity system. Repos have minimal CLAUDE.md (build commands only).
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
    /// PID of the currently running child process (0 = none). Shared with supervisor
    /// so it can kill the process group on timeout.
    pub child_pid: Arc<AtomicU32>,
    /// Max retries on transient failures.
    max_retries: u32,
    /// Optional cost callback — invoked when the stream reports cost.
    /// Return false to abort.
    cost_callback: Option<CostCallback>,
    /// Optional progress sender — emits tool/turn progress during streaming.
    progress_sender: Option<tokio::sync::watch::Sender<ExecutionProgress>>,
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
            child_pid: Arc::new(AtomicU32::new(0)),
            max_retries: 3,
            cost_callback: None,
            progress_sender: None,
        }
    }

    /// Set the max retries for transient failures.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set a cost callback for budget enforcement once the stream reports cost.
    pub fn with_cost_callback(mut self, cb: CostCallback) -> Self {
        self.cost_callback = Some(cb);
        self
    }

    /// Set a progress sender for real-time execution visibility.
    /// Returns the watch receiver for the caller to subscribe to.
    pub fn with_progress_channel(
        mut self,
    ) -> (Self, tokio::sync::watch::Receiver<ExecutionProgress>) {
        let (tx, rx) = tokio::sync::watch::channel(ExecutionProgress::default());
        self.progress_sender = Some(tx);
        (self, rx)
    }

    /// Get the working directory for this executor.
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// Execute a task via Claude Code CLI with retry on transient failures.
    ///
    /// Spawns `claude -p "<task_context>"` with the project's identity + worker protocol
    /// as `--append-system-prompt`. Retries up to self.max_retries times with exponential
    /// backoff on spawn failures or non-zero exit codes.
    pub async fn execute(
        &self,
        identity: &sigil_core::Identity,
        task_context: &str,
    ) -> Result<ExecutionResult> {
        let system_prompt = {
            let budget = crate::context_budget::ContextBudget::default();
            let mut sp = budget.apply_to_identity(identity);
            sp.push_str("\n\n---\n\n");
            sp.push_str(WORKER_PROTOCOL);
            sp
        };

        let mut last_err = None;

        for attempt in 1..=self.max_retries {
            match self.execute_once(&system_prompt, task_context).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let is_parse_error = e.to_string().contains("failed to parse claude code JSON");
                    if is_parse_error || attempt == self.max_retries {
                        return Err(e);
                    }
                    let delay = INITIAL_RETRY_DELAY_MS * 2u64.pow(attempt - 1);
                    warn!(
                        attempt,
                        max = self.max_retries,
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

    /// Single execution attempt (no retry). Uses `--output-format stream-json`
    /// for progress visibility and final-result cost inspection.
    async fn execute_once(
        &self,
        system_prompt: &str,
        task_context: &str,
    ) -> Result<ExecutionResult> {
        let start = Instant::now();

        let claude_bin = resolve_claude_binary();
        let mut cmd = Command::new(&claude_bin);

        cmd.arg("-p").arg(task_context);
        cmd.arg("--output-format").arg("stream-json");
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

        // Capture stdout for streaming, stderr for errors.
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Create a new process group so we can kill the entire tree on timeout.
        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                // setsid() creates a new session + process group.
                unsafe extern "C" {
                    fn setsid() -> i32;
                }
                let _ = setsid();
                Ok(())
            });
        }

        debug!(
            workdir = %self.workdir.display(),
            model = %self.model,
            max_turns = self.max_turns,
            "spawning claude code (streaming)"
        );

        let timeout_secs = (self.max_turns as u64 * 300).max(1800);

        // Spawn child and track its PID for process group kill.
        let mut child = cmd
            .spawn()
            .context("failed to spawn claude CLI — is it installed?")?;
        let pid = child.id().unwrap_or(0) as u32;
        self.child_pid.store(pid, Ordering::Relaxed);

        // Read stdout line-by-line for streaming events.
        let stdout = child
            .stdout
            .take()
            .context("failed to capture stdout from claude CLI")?;
        let mut reader = BufReader::new(stdout).lines();

        let mut final_result: Option<ExecutionResult> = None;
        let mut progress = ExecutionProgress::default();
        let mut aborted = false;

        let stream_result =
            tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
                while let Some(line) = reader.next_line().await? {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }

                    // Try to parse as a stream event. Skip unparseable lines
                    // (e.g. init messages, progress indicators).
                    let event: StreamEvent = match serde_json::from_str(&line) {
                        Ok(e) => e,
                        Err(_) => continue,
                    };

                    match event {
                        StreamEvent::Result {
                            result,
                            session_id,
                            num_turns,
                            total_cost_usd,
                            duration_ms,
                        } => {
                            let dur = if duration_ms > 0 {
                                duration_ms
                            } else {
                                start.elapsed().as_millis() as u64
                            };
                            final_result = Some(ExecutionResult {
                                result_text: result,
                                session_id,
                                num_turns,
                                total_cost_usd,
                                duration_ms: dur,
                            });
                        }
                        StreamEvent::ToolUse { ref name } => {
                            progress.last_tool = Some(name.clone());
                            progress.turns_so_far += 1;
                            if let Some(ref tx) = self.progress_sender {
                                let _ = tx.send(progress.clone());
                            }
                        }
                        StreamEvent::System { .. }
                        | StreamEvent::Assistant { .. }
                        | StreamEvent::ToolResult { .. } => {}
                    }

                    // Check cost callback on every event that might update cost.
                    if let Some(ref result) = final_result {
                        progress.cost_so_far = result.total_cost_usd;
                    }
                    if let Some(ref cb) = self.cost_callback
                        && progress.cost_so_far > 0.0
                        && !cb(progress.cost_so_far)
                    {
                        warn!(
                            cost_usd = progress.cost_so_far,
                            "cost callback triggered abort"
                        );
                        aborted = true;
                        Self::kill_process_group(pid);
                        anyhow::bail!(
                            "execution aborted by cost callback at ${:.4}",
                            progress.cost_so_far
                        );
                    }
                }
                Ok::<(), anyhow::Error>(())
            })
            .await;

        // Wait for the child process to exit.
        let status = child.wait().await;
        self.child_pid.store(0, Ordering::Relaxed);
        let duration_ms = start.elapsed().as_millis() as u64;

        match stream_result {
            Err(_) => {
                // Timeout
                Self::kill_process_group(pid);
                anyhow::bail!(
                    "claude code timed out after {}s (max_turns={})",
                    timeout_secs,
                    self.max_turns,
                );
            }
            Ok(Err(e)) => {
                // Stream processing error (including cost abort).
                return Err(e);
            }
            Ok(Ok(())) => {}
        }

        if aborted {
            anyhow::bail!("execution aborted by cost callback");
        }

        // Check exit status — if we got no result event and the process failed, report it.
        if let Ok(status) = status
            && !status.success()
            && final_result.is_none()
        {
            anyhow::bail!("claude code failed (exit {})", status.code().unwrap_or(-1),);
        }

        match final_result {
            Some(mut result) => {
                if result.duration_ms == 0 {
                    result.duration_ms = duration_ms;
                }
                info!(
                    turns = result.num_turns,
                    cost_usd = result.total_cost_usd,
                    duration_ms = result.duration_ms,
                    result_len = result.result_text.len(),
                    "claude code execution complete"
                );
                Ok(result)
            }
            None => {
                anyhow::bail!("claude code stream ended without a result event");
            }
        }
    }

    /// Kill an entire process group by PID. Used on timeout to clean up
    /// Claude Code and any sub-processes it spawned.
    pub fn kill_process_group(pid: u32) {
        if pid == 0 {
            return;
        }
        #[cfg(unix)]
        {
            // kill(-pid, SIGKILL) sends SIGKILL to the entire process group.
            unsafe extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            const SIGKILL: i32 = 9;
            unsafe {
                kill(-(pid as i32), SIGKILL);
            }
            info!(pid, "killed process group");
        }
    }

    /// Parse the `--output-format json` response from Claude Code.
    /// Kept for tests and potential non-streaming fallback.
    #[allow(dead_code)]
    fn parse_json_output(stdout: &str, duration_ms: u64) -> Result<ExecutionResult> {
        let v: serde_json::Value =
            serde_json::from_str(stdout).context("failed to parse claude code JSON output")?;

        let result_text = v
            .get("result")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string();

        let session_id = v
            .get("session_id")
            .and_then(|s| s.as_str())
            .map(String::from);

        let num_turns = v.get("num_turns").and_then(|n| n.as_u64()).unwrap_or(0) as u32;

        let cost_missing = v.get("total_cost_usd").is_none();
        let total_cost_usd = v
            .get("total_cost_usd")
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
    /// Worker is blocked and needs input to continue.
    Blocked {
        /// The specific question or information needed.
        question: String,
        /// Full result text including work done so far.
        full_text: String,
    },
    /// Worker hit context exhaustion but made progress. Re-queue with checkpoint.
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

        // Empty or whitespace-only responses are failures — never silently mark done.
        if trimmed.is_empty() {
            return Self::Failed("Worker returned empty response".to_string());
        }

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
                trimmed.strip_prefix("BLOCKED:").unwrap_or(trimmed).trim()
            } else {
                // Prefix + text on same line.
                first_line.strip_prefix("BLOCKED:").unwrap_or("").trim()
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
        let outcome =
            TaskOutcome::parse("FAILED:\ncargo build returned 3 errors in pms/src/main.rs");
        assert!(matches!(outcome, TaskOutcome::Failed(_)));
    }

    #[test]
    fn test_worker_outcome_empty_is_failed() {
        let outcome = TaskOutcome::parse("");
        assert!(matches!(outcome, TaskOutcome::Failed(_)));

        let outcome = TaskOutcome::parse("   \n  \n  ");
        assert!(matches!(outcome, TaskOutcome::Failed(_)));
    }

    #[test]
    fn test_worker_outcome_handoff() {
        let text = "HANDOFF:\nCompleted: Implemented debounce buffer.\nRemaining: Wire into daemon dispatch loop.";
        let outcome = TaskOutcome::parse(text);
        match outcome {
            TaskOutcome::Handoff { checkpoint } => {
                assert!(checkpoint.contains("Completed: Implemented debounce buffer"));
                assert!(checkpoint.contains("Remaining:"));
            }
            _ => panic!("expected Handoff"),
        }
    }

    #[test]
    fn test_parse_stream_events() {
        // Verify StreamEvent deserialization for each variant.
        let system = r#"{"type":"system","subtype":"init"}"#;
        let event: StreamEvent = serde_json::from_str(system).unwrap();
        assert!(matches!(event, StreamEvent::System { .. }));

        let assistant = r#"{"type":"assistant","message":{"content":"hello"}}"#;
        let event: StreamEvent = serde_json::from_str(assistant).unwrap();
        assert!(matches!(event, StreamEvent::Assistant { .. }));

        let tool_use = r#"{"type":"tool_use","name":"Read"}"#;
        let event: StreamEvent = serde_json::from_str(tool_use).unwrap();
        match event {
            StreamEvent::ToolUse { name } => assert_eq!(name, "Read"),
            _ => panic!("expected ToolUse"),
        }

        let result = r#"{"type":"result","result":"done","session_id":"s1","num_turns":5,"total_cost_usd":0.12,"duration_ms":3000}"#;
        let event: StreamEvent = serde_json::from_str(result).unwrap();
        match event {
            StreamEvent::Result {
                result,
                num_turns,
                total_cost_usd,
                ..
            } => {
                assert_eq!(result, "done");
                assert_eq!(num_turns, 5);
                assert!((total_cost_usd - 0.12).abs() < f64::EPSILON);
            }
            _ => panic!("expected Result"),
        }
    }

    #[test]
    fn test_cost_callback_abort_logic() {
        // Verify the CostCallback type and closure behavior.
        let cb: CostCallback = Arc::new(|cost| cost < 1.0);
        assert!(cb(0.5)); // under budget
        assert!(!cb(1.5)); // over budget → should abort
    }

    #[test]
    fn test_execution_progress_default() {
        let p = ExecutionProgress::default();
        assert_eq!(p.turns_so_far, 0);
        assert_eq!(p.cost_so_far, 0.0);
        assert!(p.last_tool.is_none());
    }
}
