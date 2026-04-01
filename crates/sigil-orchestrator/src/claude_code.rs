use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

pub struct ClaudeCodeResult {
    pub text: String,
    pub cost_usd: f64,
    pub num_turns: u32,
    pub session_id: String,
    pub model: String,
}

pub struct ClaudeCodeExecutor {
    cwd: PathBuf,
    max_budget_usd: f64,
    session_id: Option<String>,
}

impl ClaudeCodeExecutor {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            max_budget_usd: 5.0,
            session_id: None,
        }
    }

    pub fn with_budget(mut self, budget: f64) -> Self {
        self.max_budget_usd = budget;
        self
    }

    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Execute with the worker's enriched identity prepended to the task prompt.
    /// Fixes the bug where ClaudeCode execution mode silently dropped the entire
    /// identity system (persona, memory, blackboard, resume brief).
    pub async fn execute_with_identity(
        &self,
        system_prompt: &str,
        task_prompt: &str,
    ) -> Result<ClaudeCodeResult> {
        let full_prompt = if system_prompt.is_empty() {
            task_prompt.to_string()
        } else {
            format!("{system_prompt}\n\n---\n\n{task_prompt}")
        };
        self.execute(&full_prompt).await
    }

    pub async fn execute(&self, prompt: &str) -> Result<ClaudeCodeResult> {
        let claude_bin = find_claude_binary()?;

        let mut cmd = Command::new(&claude_bin);
        cmd.arg("--print")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--dangerously-skip-permissions")
            .arg("--max-budget-usd")
            .arg(self.max_budget_usd.to_string())
            .arg("--verbose")
            .current_dir(&self.cwd);

        if let Some(ref sid) = self.session_id {
            cmd.arg("--continue").arg(sid);
        }

        cmd.arg("--prompt").arg(prompt);

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        info!(
            cwd = %self.cwd.display(),
            session = ?self.session_id,
            budget = self.max_budget_usd,
            "spawning claude code"
        );

        let mut child = cmd.spawn().context("failed to spawn claude CLI")?;
        let stdout = child.stdout.take().context("no stdout")?;

        let mut reader = BufReader::new(stdout).lines();
        let mut result_text = String::new();
        let mut cost_usd = 0.0;
        let mut num_turns = 0u32;
        let mut session_id = String::new();
        let mut model = String::from("claude-code");

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }

            let event: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => {
                    debug!(line = %line, "skipping non-JSON line");
                    continue;
                }
            };

            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");

            match event_type {
                "system" => {
                    if let Some(sid) = event.get("session_id").and_then(|v| v.as_str()) {
                        session_id = sid.to_string();
                    }
                    if let Some(m) = event.get("model").and_then(|v| v.as_str()) {
                        model = m.to_string();
                    }
                    debug!(session = %session_id, "claude code session started");
                }
                "assistant" => {
                    if let Some(content) = event.get("message").and_then(|v| v.get("content")) {
                        if let Some(arr) = content.as_array() {
                            for block in arr {
                                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                                    if let Some(text) = block.get("text").and_then(|v| v.as_str())
                                    {
                                        result_text = text.to_string();
                                    }
                                }
                            }
                        }
                    }
                    num_turns += 1;
                }
                "result" => {
                    if let Some(c) = event.get("cost_usd").and_then(|v| v.as_f64()) {
                        cost_usd = c;
                    }
                    if let Some(t) = event.get("num_turns").and_then(|v| v.as_u64()) {
                        num_turns = t as u32;
                    }
                    if let Some(sid) = event.get("session_id").and_then(|v| v.as_str()) {
                        session_id = sid.to_string();
                    }
                    if let Some(r) = event.get("result").and_then(|v| v.as_str()) {
                        result_text = r.to_string();
                    }
                    debug!(
                        cost = cost_usd,
                        turns = num_turns,
                        "claude code execution complete"
                    );
                }
                _ => {
                    debug!(event_type, "claude code event");
                }
            }
        }

        let status = child.wait().await?;
        if !status.success() {
            let code = status.code().unwrap_or(-1);
            warn!(exit_code = code, "claude code exited with error");
            if result_text.is_empty() {
                result_text = format!("FAILED: claude code exited with code {code}");
            }
        }

        Ok(ClaudeCodeResult {
            text: result_text,
            cost_usd,
            num_turns,
            session_id,
            model,
        })
    }
}

fn find_claude_binary() -> Result<PathBuf> {
    let candidates = [
        PathBuf::from("/home/claudedev/.local/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
        PathBuf::from("/usr/bin/claude"),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    which_claude().context("claude CLI not found. Install Claude Code first.")
}

fn which_claude() -> Result<PathBuf> {
    let output = std::process::Command::new("which")
        .arg("claude")
        .output()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    anyhow::bail!("claude not in PATH")
}
