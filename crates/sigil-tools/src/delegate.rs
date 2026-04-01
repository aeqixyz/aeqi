use anyhow::Result;
use async_trait::async_trait;
use sigil_core::traits::{LogObserver, Observer, Provider, Tool, ToolResult, ToolSpec};
use sigil_core::{Agent, AgentConfig, Identity, LoopNotification, NotificationSender, SessionType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

// ---------------------------------------------------------------------------
// Agent notification — delivered to parent after background agent completes
// ---------------------------------------------------------------------------

/// Notification emitted when a background agent completes.
#[derive(Debug, Clone)]
pub struct AgentNotification {
    pub agent_id: String,
    pub description: String,
    pub status: AgentNotificationStatus,
    pub result_text: Option<String>,
    pub total_tokens: u32,
    pub iterations: u32,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentNotificationStatus {
    Completed,
    Failed,
}

impl AgentNotification {
    /// Format as XML task-notification (CC-compatible protocol).
    pub fn to_xml(&self) -> String {
        let status = match self.status {
            AgentNotificationStatus::Completed => "completed",
            AgentNotificationStatus::Failed => "failed",
        };
        let result_block = self
            .result_text
            .as_ref()
            .map(|t| format!("<result>{t}</result>\n"))
            .unwrap_or_default();
        format!(
            "<task-notification>\n\
             <task-id>{}</task-id>\n\
             <status>{status}</status>\n\
             <summary>Agent \"{}\" {status}</summary>\n\
             {result_block}\
             <usage>\n  <total_tokens>{}</total_tokens>\n  \
             <iterations>{}</iterations>\n  \
             <duration_ms>{}</duration_ms>\n</usage>\n\
             </task-notification>",
            self.agent_id,
            self.description,
            self.total_tokens,
            self.iterations,
            self.duration_ms,
        )
    }
}

// ---------------------------------------------------------------------------
// Agent handle — tracks a running/completed background agent
// ---------------------------------------------------------------------------

/// Status of a background agent.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Running,
    Completed,
    Failed,
}

/// Handle to a background agent for registry tracking.
#[derive(Debug)]
pub struct AgentHandle {
    pub id: String,
    pub description: String,
    pub status: AgentStatus,
    /// Set once notification is sent — prevents duplicate delivery.
    pub(crate) notified: bool,
}

// ---------------------------------------------------------------------------
// Agent registry — shared state for background agent tracking
// ---------------------------------------------------------------------------

/// Shared registry of background agents + notification channel.
///
/// `notification_tx` delivers `AgentNotification` for the delegate tool's tracking.
/// `loop_tx` delivers `LoopNotification` to the parent agent's loop for injection
/// as user-role messages between turns.
#[derive(Clone)]
pub struct AgentInfra {
    pub registry: Arc<Mutex<HashMap<String, AgentHandle>>>,
    pub notification_tx: mpsc::UnboundedSender<AgentNotification>,
    /// Sends XML notifications into the parent agent loop.
    pub loop_tx: NotificationSender,
}

impl AgentInfra {
    /// Create infrastructure with a pre-existing loop notification sender.
    /// The receiver should be passed to `Agent::with_notification_rx()`.
    pub fn new(loop_tx: NotificationSender) -> (Self, mpsc::UnboundedReceiver<AgentNotification>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                registry: Arc::new(Mutex::new(HashMap::new())),
                notification_tx: tx,
                loop_tx,
            },
            rx,
        )
    }
}

// ---------------------------------------------------------------------------
// DelegateTool — spawns sub-agents (sync or async)
// ---------------------------------------------------------------------------

/// Tool for spawning a sub-agent with a delegated task.
///
/// Supports two modes:
/// - **Sync** (default): Blocks the parent until the sub-agent completes.
/// - **Async** (`run_in_background: true`): Spawns the agent in the background,
///   returns immediately with an agent ID. The parent is notified via an XML
///   `<task-notification>` message when the agent completes.
///
/// Recursion prevention:
/// - **Perpetual session**: Can delegate freely (subagents are Async, cannot re-delegate)
/// - **Async session**: Can delegate but subagents get NO delegate tool
/// - **Subagent** (depth > 0): Delegation blocked entirely
pub struct DelegateTool {
    provider: Arc<dyn Provider>,
    tools: Vec<Arc<dyn Tool>>,
    identity: Identity,
    model: String,
    session_type: SessionType,
    /// Delegation depth. 0 = top-level session. >0 = already a subagent.
    depth: u32,
    /// Shared infrastructure for background agents. None = async mode unavailable.
    infra: Option<AgentInfra>,
}

impl DelegateTool {
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        identity: Identity,
        model: String,
    ) -> Self {
        Self {
            provider,
            tools,
            identity,
            model,
            session_type: SessionType::Async,
            depth: 0,
            infra: None,
        }
    }

    pub fn with_session_type(mut self, session_type: SessionType) -> Self {
        self.session_type = session_type;
        self
    }

    pub fn with_depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    /// Enable async (background) delegation with shared infrastructure.
    pub fn with_infra(mut self, infra: AgentInfra) -> Self {
        self.infra = Some(infra);
        self
    }

    fn build_tools(&self, args: &serde_json::Value) -> Vec<Arc<dyn Tool>> {
        let mut tools: Vec<Arc<dyn Tool>> =
            if let Some(allow) = args.get("tools").and_then(|v| v.as_array()) {
                let allowed: Vec<String> = allow
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                self.tools
                    .iter()
                    .filter(|t| allowed.contains(&t.name().to_string()))
                    .cloned()
                    .collect()
            } else {
                self.tools.clone()
            };

        // Remove delegate tool from subagent to enforce flat execution graph.
        tools.retain(|t| t.name() != "delegate");
        tools
    }

    fn generate_agent_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        // 8-char hex from timestamp + pid for uniqueness.
        format!("a-{:08x}{:04x}", ts as u32, std::process::id() as u16)
    }

    async fn run_sync(&self, args: &serde_json::Value) -> Result<ToolResult> {
        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing prompt"))?;
        let max_iterations = args
            .get("max_iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;
        let agent_name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("delegate");

        let tools = self.build_tools(args);
        let observer: Arc<dyn Observer> = Arc::new(LogObserver);
        let config = AgentConfig {
            model: self.model.clone(),
            max_iterations,
            name: agent_name.to_string(),
            session_type: SessionType::Async,
            ..Default::default()
        };

        let agent = Agent::new(
            config,
            self.provider.clone(),
            tools,
            observer,
            self.identity.clone(),
        );

        match agent.run(prompt).await {
            Ok(result) => Ok(ToolResult::success(result.text)),
            Err(e) => Ok(ToolResult::error(format!("Sub-agent failed: {e}"))),
        }
    }

    async fn run_async(&self, args: &serde_json::Value) -> Result<ToolResult> {
        let infra = self
            .infra
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("async delegation not available (no infra)"))?;

        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing prompt"))?
            .to_string();
        let max_iterations = args
            .get("max_iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                args.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("background agent")
            })
            .to_string();

        let agent_id = Self::generate_agent_id();
        let tools = self.build_tools(args);

        // Register in the shared registry.
        {
            let mut reg = infra.registry.lock().await;
            reg.insert(
                agent_id.clone(),
                AgentHandle {
                    id: agent_id.clone(),
                    description: description.clone(),
                    status: AgentStatus::Running,
                    notified: false,
                },
            );
        }

        // Spawn background execution.
        let provider = self.provider.clone();
        let identity = self.identity.clone();
        let model = self.model.clone();
        let registry = infra.registry.clone();
        let notification_tx = infra.notification_tx.clone();
        let loop_tx = infra.loop_tx.clone();
        let id = agent_id.clone();
        let desc = description.clone();

        tokio::spawn(async move {
            let start = std::time::Instant::now();
            let observer: Arc<dyn Observer> = Arc::new(LogObserver);
            let config = AgentConfig {
                model,
                max_iterations,
                name: desc.clone(),
                session_type: SessionType::Async,
                ..Default::default()
            };

            let agent = Agent::new(config, provider, tools, observer, identity);
            let result = agent.run(&prompt).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            // Build notification.
            let notification = match &result {
                Ok(r) => AgentNotification {
                    agent_id: id.clone(),
                    description: desc.clone(),
                    status: AgentNotificationStatus::Completed,
                    result_text: Some(r.text.clone()),
                    total_tokens: r.total_prompt_tokens + r.total_completion_tokens,
                    iterations: r.iterations,
                    duration_ms,
                },
                Err(e) => AgentNotification {
                    agent_id: id.clone(),
                    description: desc.clone(),
                    status: AgentNotificationStatus::Failed,
                    result_text: Some(e.to_string()),
                    total_tokens: 0,
                    iterations: 0,
                    duration_ms,
                },
            };

            // Update registry + send notification (atomic dedup).
            {
                let mut reg = registry.lock().await;
                if let Some(handle) = reg.get_mut(&id) {
                    handle.status = match &result {
                        Ok(_) => AgentStatus::Completed,
                        Err(_) => AgentStatus::Failed,
                    };
                    if !handle.notified {
                        handle.notified = true;
                        // Send to delegate tool's tracking channel.
                        let _ = notification_tx.send(notification.clone());
                        // Send XML to parent agent loop for injection between turns.
                        let _ = loop_tx.send(LoopNotification {
                            content: notification.to_xml(),
                        });
                    }
                }
            }
        });

        Ok(ToolResult::success(format!(
            "Background agent \"{}\" launched with ID {}. You will be notified when it completes. \
             Do not wait — continue with other work.",
            description, agent_id
        )))
    }
}

#[async_trait]
impl Tool for DelegateTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        // Block delegation from subagents (prevent recursion).
        if self.depth > 0 {
            return Ok(ToolResult::error(
                "Cannot delegate from a sub-agent. Only top-level sessions can delegate.",
            ));
        }

        let run_in_background = args
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if run_in_background {
            self.run_async(&args).await
        } else {
            self.run_sync(&args).await
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "delegate".to_string(),
            description: "Spawn a sub-agent to handle a delegated task. Use run_in_background=true \
                for tasks that can run independently while you continue other work. The sub-agent \
                runs with the same tools and identity but its own iteration budget."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Task for the sub-agent" },
                    "description": { "type": "string", "description": "Short description of what the agent will do (3-5 words)" },
                    "name": { "type": "string", "description": "Sub-agent name for logging", "default": "delegate" },
                    "max_iterations": { "type": "integer", "description": "Max tool-call iterations", "default": 10 },
                    "run_in_background": { "type": "boolean", "description": "Run asynchronously — returns immediately, notifies on completion", "default": false },
                    "tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional tool allowlist. If omitted, all tools available."
                    }
                },
                "required": ["prompt"]
            }),
        }
    }

    fn name(&self) -> &str {
        "delegate"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_notification_xml() {
        let notif = AgentNotification {
            agent_id: "a-12345678abcd".to_string(),
            description: "Research auth module".to_string(),
            status: AgentNotificationStatus::Completed,
            result_text: Some("Found the bug in validate.ts".to_string()),
            total_tokens: 5000,
            iterations: 3,
            duration_ms: 12000,
        };
        let xml = notif.to_xml();
        assert!(xml.contains("<task-id>a-12345678abcd</task-id>"));
        assert!(xml.contains("<status>completed</status>"));
        assert!(xml.contains("<result>Found the bug in validate.ts</result>"));
        assert!(xml.contains("<total_tokens>5000</total_tokens>"));
    }

    #[test]
    fn test_agent_notification_failed() {
        let notif = AgentNotification {
            agent_id: "a-00000000ffff".to_string(),
            description: "Fix the bug".to_string(),
            status: AgentNotificationStatus::Failed,
            result_text: Some("Timeout".to_string()),
            total_tokens: 0,
            iterations: 0,
            duration_ms: 30000,
        };
        let xml = notif.to_xml();
        assert!(xml.contains("<status>failed</status>"));
        assert!(xml.contains("<result>Timeout</result>"));
    }

    #[test]
    fn test_generate_agent_id() {
        let id = DelegateTool::generate_agent_id();
        assert!(id.starts_with("a-"));
        assert!(id.len() >= 10);
    }

    #[test]
    fn test_agent_infra_creation() {
        let (loop_tx, _loop_rx) = tokio::sync::mpsc::unbounded_channel();
        let (infra, _rx) = AgentInfra::new(loop_tx);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let reg = infra.registry.lock().await;
            assert!(reg.is_empty());
        });
    }
}
