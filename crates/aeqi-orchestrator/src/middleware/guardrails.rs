//! Guardrails Middleware — tiered permission system for tool calls.
//!
//! Three tiers:
//! - **Allow**: Known-safe patterns that always pass (read-only tools, safe commands).
//! - **Deny**: Dangerous patterns that always halt (destructive commands, data loss).
//! - **Ask**: Everything else — passes in autonomous mode, injects a caution warning
//!   in supervised mode to let the model self-review before proceeding.
//!
//! The existing deny list is preserved as the deny tier. The allow list covers
//! read-only tools and safe command patterns. The ask tier is the default for
//! unmatched calls.

use async_trait::async_trait;
use tracing::{debug, warn};

use super::{Middleware, MiddlewareAction, ORDER_GUARDRAILS, ToolCall, WorkerContext};

/// Permission tier for a tool call.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionTier {
    /// Always allowed — no checks needed.
    Allow,
    /// Requires review in supervised mode, auto-allowed in autonomous mode.
    Ask,
    /// Always blocked.
    Deny(String),
}

/// A pattern that matches tool calls.
#[derive(Debug, Clone)]
pub struct ToolPattern {
    /// The string pattern to search for (case-insensitive substring match).
    pub pattern: String,
    /// Human-readable reason for the classification.
    pub reason: String,
    /// Which tier this pattern belongs to.
    pub tier: PermissionTier,
}

impl ToolPattern {
    pub fn deny(pattern: impl Into<String>, reason: impl Into<String>) -> Self {
        let reason_str: String = reason.into();
        Self {
            pattern: pattern.into().to_lowercase(),
            reason: reason_str.clone(),
            tier: PermissionTier::Deny(reason_str),
        }
    }

    pub fn allow(pattern: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into().to_lowercase(),
            reason: reason.into(),
            tier: PermissionTier::Allow,
        }
    }
}

/// Backwards-compatible type alias.
pub type DenyPattern = ToolPattern;

impl DenyPattern {
    pub fn new(pattern: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::deny(pattern, reason)
    }
}

/// Execution mode that determines how "ask" tier calls are handled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutionMode {
    /// Agent runs autonomously — ask-tier calls pass silently.
    Autonomous,
    /// Agent is supervised — ask-tier calls inject a caution message.
    Supervised,
}

/// Guardrails middleware with tiered permissions.
pub struct GuardrailsMiddleware {
    patterns: Vec<ToolPattern>,
    mode: ExecutionMode,
}

impl GuardrailsMiddleware {
    /// Create with explicit patterns and mode.
    pub fn new(deny_patterns: Vec<DenyPattern>) -> Self {
        Self {
            patterns: deny_patterns,
            mode: ExecutionMode::Autonomous,
        }
    }

    /// Create with tiered patterns and mode.
    pub fn tiered(patterns: Vec<ToolPattern>, mode: ExecutionMode) -> Self {
        Self { patterns, mode }
    }

    /// Create with a sensible set of default patterns (all tiers).
    pub fn with_defaults() -> Self {
        let mut patterns = Self::default_deny_patterns();
        patterns.extend(Self::default_allow_patterns());
        Self {
            patterns,
            mode: ExecutionMode::Autonomous,
        }
    }

    /// Create with defaults in the specified mode.
    pub fn with_defaults_mode(mode: ExecutionMode) -> Self {
        let mut patterns = Self::default_deny_patterns();
        patterns.extend(Self::default_allow_patterns());
        Self { patterns, mode }
    }

    fn default_deny_patterns() -> Vec<ToolPattern> {
        vec![
            ToolPattern::deny("rm -rf /", "Recursive deletion of root filesystem"),
            ToolPattern::deny("rm -rf ~", "Recursive deletion of home directory"),
            ToolPattern::deny("rm -rf *", "Wildcard recursive deletion"),
            ToolPattern::deny(
                "git push --force",
                "Force push — use --force-with-lease if necessary",
            ),
            ToolPattern::deny(
                "git push -f",
                "Force push — use --force-with-lease if necessary",
            ),
            ToolPattern::deny("DROP TABLE", "SQL DROP TABLE"),
            ToolPattern::deny("DROP DATABASE", "SQL DROP DATABASE"),
            ToolPattern::deny("TRUNCATE TABLE", "SQL TRUNCATE TABLE"),
            ToolPattern::deny(":(){ :|:& };:", "Fork bomb"),
            ToolPattern::deny("mkfs.", "Filesystem formatting"),
            ToolPattern::deny("dd if=/dev/zero", "Disk overwrite with dd"),
            ToolPattern::deny("> /dev/sda", "Direct disk device write"),
            ToolPattern::deny("chmod -R 777", "Recursive world-writable permissions"),
        ]
    }

    fn default_allow_patterns() -> Vec<ToolPattern> {
        vec![
            // Read-only tools are always safe.
            ToolPattern::allow("Read", "Read-only file access"),
            ToolPattern::allow("Glob", "File pattern matching"),
            ToolPattern::allow("Grep", "Content search"),
            ToolPattern::allow("aeqi_recall", "Memory search"),
            ToolPattern::allow("aeqi_graph", "Code graph query"),
            ToolPattern::allow("aeqi_status", "Status check"),
            ToolPattern::allow("aeqi_skills", "Skill loading"),
            ToolPattern::allow("aeqi_notes", "Notes read"),
            ToolPattern::allow("aeqi_agents", "Agent listing"),
            // Safe git commands.
            ToolPattern::allow("git status", "Git status check"),
            ToolPattern::allow("git log", "Git log view"),
            ToolPattern::allow("git diff", "Git diff view"),
            ToolPattern::allow("git branch", "Git branch list"),
            ToolPattern::allow("cargo test", "Test execution"),
            ToolPattern::allow("cargo check", "Compilation check"),
            ToolPattern::allow("cargo clippy", "Lint check"),
        ]
    }

    /// Classify a tool call into a permission tier.
    fn classify(&self, call: &ToolCall) -> PermissionTier {
        let name_lower = call.name.to_lowercase();
        let input_lower = call.input.to_lowercase();
        let combined = format!("{name_lower} {input_lower}");

        // Check deny patterns first (highest priority).
        for p in &self.patterns {
            if matches!(p.tier, PermissionTier::Deny(_)) && combined.contains(&p.pattern) {
                return p.tier.clone();
            }
        }

        // Check allow patterns.
        for p in &self.patterns {
            if p.tier == PermissionTier::Allow && combined.contains(&p.pattern) {
                return PermissionTier::Allow;
            }
        }

        // Default: ask tier.
        PermissionTier::Ask
    }
}

#[async_trait]
impl Middleware for GuardrailsMiddleware {
    fn name(&self) -> &str {
        "guardrails"
    }

    fn order(&self) -> u32 {
        ORDER_GUARDRAILS
    }

    async fn before_tool(&self, _ctx: &mut WorkerContext, call: &ToolCall) -> MiddlewareAction {
        match self.classify(call) {
            PermissionTier::Allow => {
                debug!(tool = %call.name, "guardrails: allowed");
                MiddlewareAction::Continue
            }
            PermissionTier::Ask => match self.mode {
                ExecutionMode::Autonomous => {
                    debug!(tool = %call.name, "guardrails: ask tier, autonomous mode — passing");
                    MiddlewareAction::Continue
                }
                ExecutionMode::Supervised => {
                    debug!(tool = %call.name, "guardrails: ask tier, supervised mode — injecting caution");
                    MiddlewareAction::Inject(vec![format!(
                        "[Guardrails] Tool '{}' is not on the allow list. \
                         Verify this action is safe before proceeding.",
                        call.name
                    )])
                }
            },
            PermissionTier::Deny(reason) => {
                warn!(
                    tool = %call.name,
                    reason = %reason,
                    "guardrails blocked dangerous tool call"
                );
                MiddlewareAction::Halt(format!(
                    "Guardrails blocked: tool '{}' matched deny pattern. Reason: {}",
                    call.name, reason
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> WorkerContext {
        WorkerContext::new("task-1", "test", "engineer", "aeqi")
    }

    #[tokio::test]
    async fn safe_command_passes() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "cargo test --workspace".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    #[tokio::test]
    async fn rm_rf_root_blocked() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "rm -rf /".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(
            matches!(action, MiddlewareAction::Halt(ref s) if s.contains("Recursive deletion")),
            "expected Halt for rm -rf /, got {action:?}"
        );
    }

    #[tokio::test]
    async fn force_push_blocked() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "git push --force origin main".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(
            matches!(action, MiddlewareAction::Halt(ref s) if s.contains("Force push")),
            "expected Halt for force push, got {action:?}"
        );
    }

    #[tokio::test]
    async fn force_push_short_flag_blocked() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "git push -f origin main".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Halt(_)));
    }

    #[tokio::test]
    async fn drop_table_blocked() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "sqlite3 db.sqlite 'DROP TABLE users;'".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(
            matches!(action, MiddlewareAction::Halt(ref s) if s.contains("DROP TABLE")),
            "expected Halt for DROP TABLE, got {action:?}"
        );
    }

    #[tokio::test]
    async fn case_insensitive_matching() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "drop table users".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Halt(_)));
    }

    #[tokio::test]
    async fn custom_deny_patterns() {
        let mw = GuardrailsMiddleware::new(vec![DenyPattern::new(
            "sudo reboot",
            "Rebooting is not allowed",
        )]);
        let mut ctx = test_ctx();

        // Blocked.
        let call = ToolCall {
            name: "Bash".into(),
            input: "sudo reboot now".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Halt(_)));

        // Not blocked (different command).
        let call = ToolCall {
            name: "Bash".into(),
            input: "sudo apt update".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    #[tokio::test]
    async fn read_tool_passes() {
        let mw = GuardrailsMiddleware::with_defaults();
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Read".into(),
            input: "/etc/passwd".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    #[tokio::test]
    async fn empty_deny_list_passes_all() {
        let mw = GuardrailsMiddleware::new(vec![]);
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Bash".into(),
            input: "rm -rf /".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    // --- New tiered permission tests ---

    #[tokio::test]
    async fn read_tool_is_allow_tier() {
        let mw = GuardrailsMiddleware::with_defaults();
        let call = ToolCall {
            name: "Read".into(),
            input: "/some/file".into(),
        };
        assert_eq!(mw.classify(&call), PermissionTier::Allow);
    }

    #[tokio::test]
    async fn glob_tool_is_allow_tier() {
        let mw = GuardrailsMiddleware::with_defaults();
        let call = ToolCall {
            name: "Glob".into(),
            input: "**/*.rs".into(),
        };
        assert_eq!(mw.classify(&call), PermissionTier::Allow);
    }

    #[tokio::test]
    async fn unknown_tool_is_ask_tier() {
        let mw = GuardrailsMiddleware::with_defaults();
        let call = ToolCall {
            name: "Write".into(),
            input: "some content".into(),
        };
        assert_eq!(mw.classify(&call), PermissionTier::Ask);
    }

    #[tokio::test]
    async fn ask_tier_passes_in_autonomous_mode() {
        let mw = GuardrailsMiddleware::with_defaults_mode(ExecutionMode::Autonomous);
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Write".into(),
            input: "some content".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Continue));
    }

    #[tokio::test]
    async fn ask_tier_injects_caution_in_supervised_mode() {
        let mw = GuardrailsMiddleware::with_defaults_mode(ExecutionMode::Supervised);
        let mut ctx = test_ctx();

        let call = ToolCall {
            name: "Write".into(),
            input: "some content".into(),
        };
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(
            matches!(action, MiddlewareAction::Inject(ref msgs) if msgs[0].contains("Guardrails")),
            "expected Inject with caution, got {action:?}"
        );
    }

    #[tokio::test]
    async fn deny_takes_priority_over_allow() {
        // git status is allowed, but git push --force is denied
        let mw = GuardrailsMiddleware::with_defaults();

        let safe_call = ToolCall {
            name: "Bash".into(),
            input: "git status".into(),
        };
        assert_eq!(mw.classify(&safe_call), PermissionTier::Allow);

        let dangerous_call = ToolCall {
            name: "Bash".into(),
            input: "git push --force origin main".into(),
        };
        assert!(matches!(
            mw.classify(&dangerous_call),
            PermissionTier::Deny(_)
        ));
    }

    #[tokio::test]
    async fn aeqi_recall_is_allow_tier() {
        let mw = GuardrailsMiddleware::with_defaults();
        let call = ToolCall {
            name: "aeqi_recall".into(),
            input: "query".into(),
        };
        assert_eq!(mw.classify(&call), PermissionTier::Allow);
    }
}
