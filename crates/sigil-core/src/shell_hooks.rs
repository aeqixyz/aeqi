//! Shell hook executor — run user-defined shell commands at agent lifecycle events.
//!
//! Hooks are shell commands configured per event. Input is passed via a temporary
//! env file (avoids argument length limits). Output is parsed as JSON if valid.
//!
//! Exit code semantics (matching Claude Code):
//! - 0: success, stdout shown in transcript only
//! - 2: block the action, stderr shown to model
//! - Other: show stderr to user, continue

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

/// Hook event types — lifecycle points where shell hooks can fire.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    SessionStart,
    SessionEnd,
    Stop,
    PreCompact,
    PostCompact,
    SubagentStart,
    SubagentStop,
    TaskCreated,
    TaskCompleted,
    UserPromptSubmit,
    FileChanged,
}

/// A shell hook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellHook {
    /// Shell command to execute.
    pub command: String,
    /// Optional pattern to match before firing (e.g., "Shell(git *)").
    #[serde(rename = "if")]
    pub if_condition: Option<String>,
    /// Timeout in milliseconds (default: 10_000).
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    10_000
}

/// Result from executing a shell hook.
#[derive(Debug, Clone)]
pub struct HookResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    /// Parsed from JSON stdout if valid.
    pub decision: Option<HookDecision>,
}

/// Decision parsed from hook's JSON stdout.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookDecision {
    /// Allow the action to proceed.
    Approve,
    /// Block the action.
    Block,
    /// Skip — no opinion, pass to next checker.
    Skip,
}

/// JSON output schema for shell hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HookJsonOutput {
    decision: Option<String>,
    reason: Option<String>,
}

/// Executor for shell hooks.
pub struct ShellHookExecutor {
    hooks: HashMap<HookEvent, Vec<ShellHook>>,
    env_dir: PathBuf,
}

impl ShellHookExecutor {
    pub fn new(hooks: HashMap<HookEvent, Vec<ShellHook>>) -> Self {
        let env_dir = std::env::temp_dir().join("sigil-hooks");
        let _ = std::fs::create_dir_all(&env_dir);
        Self { hooks, env_dir }
    }

    /// Execute all hooks for an event. Returns results in order.
    pub async fn execute(
        &self,
        event: &HookEvent,
        input: &serde_json::Value,
    ) -> Vec<HookResult> {
        let Some(hooks) = self.hooks.get(event) else {
            return Vec::new();
        };

        let mut results = Vec::new();
        for hook in hooks {
            if let Some(ref condition) = hook.if_condition
                && !self.matches_condition(condition, input)
            {
                continue;
            }
            results.push(self.execute_hook(hook, input).await);
        }
        results
    }

    async fn execute_hook(&self, hook: &ShellHook, input: &serde_json::Value) -> HookResult {
        let env_file = self.env_dir.join(format!(
            "hook-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));

        if let Err(e) = std::fs::write(&env_file, input.to_string()) {
            return HookResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("Failed to write hook input: {e}"),
                decision: None,
            };
        }

        let timeout = Duration::from_millis(hook.timeout_ms);
        let result = tokio::time::timeout(timeout, async {
            Command::new("bash")
                .arg("-c")
                .arg(&hook.command)
                .env("SIGIL_HOOK_INPUT_FILE", &env_file)
                .env("SIGIL_HOOK_INPUT", input.to_string())
                .output()
                .await
        })
        .await;

        let _ = std::fs::remove_file(&env_file);

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                let decision = serde_json::from_str::<HookJsonOutput>(&stdout)
                    .ok()
                    .and_then(|p| {
                        p.decision.map(|d| match d.as_str() {
                            "approve" | "allow" => HookDecision::Approve,
                            "block" | "deny" => HookDecision::Block,
                            _ => HookDecision::Skip,
                        })
                    });

                debug!(command = %hook.command, exit_code, "hook executed");
                HookResult { exit_code, stdout, stderr, decision }
            }
            Ok(Err(e)) => HookResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("Failed to execute hook: {e}"),
                decision: None,
            },
            Err(_) => {
                warn!(command = %hook.command, timeout_ms = hook.timeout_ms, "hook timed out");
                HookResult {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: format!("Hook timed out after {}ms", hook.timeout_ms),
                    decision: None,
                }
            }
        }
    }

    /// Simple pattern matching: "ToolName" or "ToolName(pattern*)".
    fn matches_condition(&self, condition: &str, input: &serde_json::Value) -> bool {
        let tool_name = input.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");

        if let Some(paren_pos) = condition.find('(') {
            let cond_tool = &condition[..paren_pos];
            if !tool_name.eq_ignore_ascii_case(cond_tool) {
                return false;
            }
            let pattern = condition[paren_pos + 1..].trim_end_matches(')').trim();
            if pattern.is_empty() || pattern == "*" {
                return true;
            }
            let tool_input = input.get("tool_input").map(|v| v.to_string()).unwrap_or_default();
            if let Some(prefix) = pattern.strip_suffix('*') {
                tool_input.contains(prefix)
            } else {
                tool_input.contains(pattern)
            }
        } else {
            tool_name.eq_ignore_ascii_case(condition)
        }
    }

    pub fn has_hooks(&self, event: &HookEvent) -> bool {
        self.hooks.get(event).is_some_and(|h| !h.is_empty())
    }

    pub fn total_hooks(&self) -> usize {
        self.hooks.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_condition_tool_name() {
        let executor = ShellHookExecutor::new(HashMap::new());
        let input = serde_json::json!({"tool_name": "Shell", "tool_input": {"command": "git status"}});
        assert!(executor.matches_condition("Shell", &input));
        assert!(executor.matches_condition("shell", &input));
        assert!(!executor.matches_condition("Read", &input));
    }

    #[test]
    fn test_matches_condition_with_pattern() {
        let executor = ShellHookExecutor::new(HashMap::new());
        let input = serde_json::json!({"tool_name": "Shell", "tool_input": {"command": "git status"}});
        assert!(executor.matches_condition("Shell(git *)", &input));
        assert!(executor.matches_condition("Shell(*)", &input));
        assert!(!executor.matches_condition("Shell(npm *)", &input));
    }

    #[test]
    fn test_has_hooks() {
        let mut hooks = HashMap::new();
        hooks.insert(HookEvent::PreToolUse, vec![ShellHook {
            command: "echo test".into(),
            if_condition: None,
            timeout_ms: 5000,
        }]);
        let executor = ShellHookExecutor::new(hooks);
        assert!(executor.has_hooks(&HookEvent::PreToolUse));
        assert!(!executor.has_hooks(&HookEvent::PostToolUse));
        assert_eq!(executor.total_hooks(), 1);
    }

    #[tokio::test]
    async fn test_execute_simple_hook() {
        let mut hooks = HashMap::new();
        hooks.insert(HookEvent::SessionStart, vec![ShellHook {
            command: "echo ok".into(),
            if_condition: None,
            timeout_ms: 5000,
        }]);
        let executor = ShellHookExecutor::new(hooks);
        let results = executor.execute(&HookEvent::SessionStart, &serde_json::json!({})).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].exit_code, 0);
        assert!(results[0].stdout.contains("ok"));
    }

    #[tokio::test]
    async fn test_execute_json_decision() {
        let mut hooks = HashMap::new();
        hooks.insert(HookEvent::PreToolUse, vec![ShellHook {
            command: r#"echo '{"decision":"approve","reason":"safe"}'"#.into(),
            if_condition: None,
            timeout_ms: 5000,
        }]);
        let executor = ShellHookExecutor::new(hooks);
        let results = executor.execute(&HookEvent::PreToolUse, &serde_json::json!({})).await;
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].decision, Some(HookDecision::Approve)));
    }

    #[tokio::test]
    async fn test_hook_timeout() {
        let mut hooks = HashMap::new();
        hooks.insert(HookEvent::PreToolUse, vec![ShellHook {
            command: "sleep 10".into(),
            if_condition: None,
            timeout_ms: 100,
        }]);
        let executor = ShellHookExecutor::new(hooks);
        let results = executor.execute(&HookEvent::PreToolUse, &serde_json::json!({})).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].exit_code, -1);
        assert!(results[0].stderr.contains("timed out"));
    }

    #[tokio::test]
    async fn test_exit_code_2_blocking() {
        let mut hooks = HashMap::new();
        hooks.insert(HookEvent::PreToolUse, vec![ShellHook {
            command: "echo 'blocked' >&2; exit 2".into(),
            if_condition: None,
            timeout_ms: 5000,
        }]);
        let executor = ShellHookExecutor::new(hooks);
        let results = executor.execute(&HookEvent::PreToolUse, &serde_json::json!({})).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].exit_code, 2);
        assert!(results[0].stderr.contains("blocked"));
    }
}
