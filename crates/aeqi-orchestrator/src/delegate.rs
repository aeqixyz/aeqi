//! Unified delegate tool — consolidates subagent spawning, dispatch sending,
//! task assignment, and channel posting into a single `delegate` tool with
//! routing determined by the `to` parameter.
//!
//! Response modes:
//! - `origin` — response injected back into the caller's conversation
//! - `perpetual` — response delivered to the caller's perpetual session
//! - `async` — fire-and-forget; caller notified on completion
//! - `department` — response posted to the department channel
//! - `none` — no response expected

use aeqi_core::traits::{Tool, ToolResult, ToolSpec};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;

use crate::SessionStore;
use crate::execution_events::{EventBroadcaster, ExecutionEvent};
use crate::message::{Dispatch, DispatchBus, DispatchKind};
use crate::registry::CompanyRegistry;
use crate::session_manager::SessionManager;

// ---------------------------------------------------------------------------
// DelegateTool
// ---------------------------------------------------------------------------

/// Unified tool for delegating work to subagents, named agents, or departments.
///
/// Routing is determined by the `to` parameter:
/// - `"subagent"` — delegate to the project-default agent (ephemeral worker)
/// - `"dept:<name>"` — post to a department conversation channel
/// - `<agent_name>` — send a DelegateRequest dispatch to a named agent
pub struct DelegateTool {
    dispatch_bus: Arc<DispatchBus>,
    /// The name of the calling agent (used as the "from" field in dispatches).
    agent_name: String,
    /// Optional company registry — AgentRegistry is read lazily at call time.
    registry: Option<Arc<CompanyRegistry>>,
    /// Project name for scoping default-agent lookups.
    project_name: Option<String>,
    /// Fallback target when no project-default agent is found (system escalation target).
    fallback_target: Option<String>,
    /// Optional event broadcaster for emitting DepartmentMessage events.
    event_broadcaster: Option<Arc<EventBroadcaster>>,
    /// Session ID of the calling agent, propagated as parent_session_id in delegations.
    session_id: Option<String>,
    /// Provider for direct session spawning (bypasses dispatch bus).
    provider: Option<Arc<dyn aeqi_core::traits::Provider>>,
    /// Session store for persisting session records.
    session_store: Option<Arc<SessionStore>>,
    /// Session manager for registering running sessions.
    session_manager: Option<Arc<SessionManager>>,
    /// Default model name for spawned child sessions.
    default_model: String,
}

impl DelegateTool {
    pub fn new(dispatch_bus: Arc<DispatchBus>, agent_name: String) -> Self {
        Self {
            dispatch_bus,
            agent_name,
            registry: None,
            project_name: None,
            fallback_target: None,
            event_broadcaster: None,
            session_id: None,
            provider: None,
            session_store: None,
            session_manager: None,
            default_model: String::new(),
        }
    }

    /// Set the event broadcaster for emitting department message events.
    pub fn with_event_broadcaster(mut self, broadcaster: Arc<EventBroadcaster>) -> Self {
        self.event_broadcaster = Some(broadcaster);
        self
    }

    /// Set the company registry for lazy agent registry lookups.
    pub fn with_registry(mut self, registry: Arc<CompanyRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Set the project name for scoping default-agent lookups.
    pub fn with_project(mut self, project_name: Option<String>) -> Self {
        self.project_name = project_name;
        self
    }

    /// Set the session ID of the calling agent. Propagated as `parent_session_id`
    /// in DelegateRequest dispatches so child workers can link their sessions.
    pub fn with_session_id(mut self, id: String) -> Self {
        self.session_id = Some(id);
        self
    }

    /// Set the provider for direct session spawning.
    pub fn with_provider(mut self, p: Arc<dyn aeqi_core::traits::Provider>) -> Self {
        self.provider = Some(p);
        self
    }

    /// Set the session store for persisting session records.
    pub fn with_session_store(mut self, ss: Arc<SessionStore>) -> Self {
        self.session_store = Some(ss);
        self
    }

    /// Set the session manager for registering running sessions.
    pub fn with_session_manager(mut self, sm: Arc<SessionManager>) -> Self {
        self.session_manager = Some(sm);
        self
    }

    /// Set the default model for spawned child sessions.
    pub fn with_default_model(mut self, m: String) -> Self {
        self.default_model = m;
        self
    }

    /// Parse a response mode string, defaulting to "origin".
    fn parse_response_mode(args: &serde_json::Value) -> String {
        args.get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("origin")
            .to_string()
    }

    /// Handle delegation to a named agent via DelegateRequest dispatch.
    async fn delegate_to_agent(
        &self,
        to: &str,
        prompt: &str,
        response_mode: &str,
        create_task: bool,
        skill: Option<String>,
    ) -> Result<ToolResult> {
        let kind = DispatchKind::DelegateRequest {
            prompt: prompt.to_string(),
            response_mode: response_mode.to_string(),
            create_task,
            skill: skill.clone(),
            reply_to: None,
            parent_session_id: self.session_id.clone(),
        };

        let dispatch = Dispatch::new_typed(&self.agent_name, to, kind);
        let dispatch_id = dispatch.id.clone();

        info!(
            from = %self.agent_name,
            to = %to,
            response_mode = %response_mode,
            create_task = create_task,
            dispatch_id = %dispatch_id,
            parent_session_id = ?self.session_id,
            "sending DelegateRequest dispatch"
        );

        self.dispatch_bus.send(dispatch).await;

        let mut msg = format!(
            "Delegation sent to '{to}' (dispatch_id: {dispatch_id}, response_mode: {response_mode})"
        );
        if create_task {
            msg.push_str("\nTask creation requested — target agent will pick up via task queue.");
        }
        if let Some(s) = &skill {
            msg.push_str(&format!("\nSkill hint: {s}"));
        }

        Ok(ToolResult::success(msg))
    }

    /// Resolve the target agent for subagent delegation.
    ///
    /// Tries the agent registry's project-default first, then falls back
    /// to the configured system escalation target.
    async fn resolve_subagent_target(&self) -> Option<String> {
        // Read AgentRegistry lazily from CompanyRegistry at call time.
        if let Some(ref company_reg) = self.registry {
            let agent_reg = company_reg.agent_registry.read().await;
            if let Some(ref agent_reg) = *agent_reg {
                // Try project-default agent.
                if let Some(ref project) = self.project_name
                    && let Ok(Some(agent)) = agent_reg.default_for_project(Some(project)).await
                {
                    info!(
                        project = %project,
                        agent = %agent.name,
                        "resolved project-default agent for subagent dispatch"
                    );
                    return Some(agent.name.clone());
                }

                // Fallback to any active agent.
                if let Ok(Some(agent)) = agent_reg.default_for_project(None).await {
                    info!(
                        agent = %agent.name,
                        "resolved fallback active agent for subagent dispatch"
                    );
                    return Some(agent.name.clone());
                }
            }
        }

        // Fall back to system escalation target.
        self.fallback_target.clone()
    }

    /// Spawn a child session directly via `tokio::spawn` — no dispatch bus or patrol.
    ///
    /// Resolves the target agent from the registry, builds a minimal `Agent`,
    /// creates a DB session record, and spawns the agent loop as a background task.
    /// The session auto-closes when the agent finishes.
    async fn spawn_session(&self, prompt: &str) -> Result<ToolResult> {
        let provider = self
            .provider
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no provider configured for direct session spawn"))?;
        let session_manager = self
            .session_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no session manager for direct session spawn"))?;

        // Resolve target agent — same logic as resolve_subagent_target but also
        // extracts system_prompt, agent_id, project, department for identity.
        let (agent_name, system_prompt, agent_id, project_name, dept_id) =
            if let Some(ref company_reg) = self.registry {
                let agent_reg = company_reg.agent_registry.read().await;
                if let Some(ref agent_reg) = *agent_reg {
                    // Try project-default agent.
                    let agent_opt = if let Some(ref project) = self.project_name {
                        agent_reg
                            .default_for_project(Some(project))
                            .await
                            .ok()
                            .flatten()
                    } else {
                        None
                    };
                    // Fallback to any active agent.
                    let agent_opt = match agent_opt {
                        Some(a) => Some(a),
                        None => agent_reg.default_for_project(None).await.ok().flatten(),
                    };
                    match agent_opt {
                        Some(agent) => (
                            agent.name.clone(),
                            agent.system_prompt.clone(),
                            Some(agent.id.clone()),
                            agent.project.clone(),
                            agent.department_id.clone(),
                        ),
                        None => (
                            self.agent_name.clone(),
                            "You are a helpful AI agent.".to_string(),
                            None,
                            self.project_name.clone(),
                            None,
                        ),
                    }
                } else {
                    (
                        self.agent_name.clone(),
                        "You are a helpful AI agent.".to_string(),
                        None,
                        self.project_name.clone(),
                        None,
                    )
                }
            } else {
                (
                    self.agent_name.clone(),
                    "You are a helpful AI agent.".to_string(),
                    None,
                    self.project_name.clone(),
                    None,
                )
            };

        // Build identity.
        let identity = aeqi_core::Identity {
            persona: Some(system_prompt),
            ..Default::default()
        };

        // Build minimal config — Async session (runs to completion, not perpetual).
        let context_window = aeqi_providers::context_window_for_model(&self.default_model);
        let config = aeqi_core::AgentConfig {
            model: self.default_model.clone(),
            max_iterations: 50,
            name: agent_name.clone(),
            context_window,
            entity_id: agent_id.clone(),
            session_type: aeqi_core::SessionType::Async,
            ..Default::default()
        };

        // Create stream sender for event broadcasting.
        let (stream_sender, _rx) = aeqi_core::chat_stream::ChatStreamSender::new(256);

        // Build agent with minimal tools — the agent can still reason without them.
        // TODO: share parent's tools or build from project config.
        let observer: Arc<dyn aeqi_core::traits::Observer> =
            Arc::new(aeqi_core::traits::LogObserver);
        let agent = aeqi_core::Agent::new(config, provider.clone(), vec![], observer, identity)
            .with_chat_stream(stream_sender.clone());

        // Create session in DB.
        let session_id = if let Some(ref ss) = self.session_store {
            ss.create_session(
                agent_id.as_deref().unwrap_or(""),
                project_name.as_deref(),
                dept_id.as_deref(),
                "delegation",
                &format!("Delegation from {}", self.agent_name),
                self.session_id.as_deref(),
                None,
            )
            .await
            .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string())
        } else {
            uuid::Uuid::new_v4().to_string()
        };

        // Record the prompt as "user" message.
        if let Some(ref ss) = self.session_store {
            let _ = ss
                .record_by_session(&session_id, "user", prompt, Some("delegation"))
                .await;
        }

        // Spawn the agent.
        let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let prompt_owned = prompt.to_string();
        let ss_clone = self.session_store.clone();
        let sid_clone = session_id.clone();
        let join_handle = tokio::spawn(async move {
            let result = agent.run(&prompt_owned).await;
            // Record result and close session.
            if let (Some(ss), Ok(r)) = (&ss_clone, &result) {
                let _ = ss
                    .record_by_session(&sid_clone, "assistant", &r.text, Some("delegation"))
                    .await;
                let _ = ss.close_session(&sid_clone).await;
            }
            result
        });

        // Register in session manager.
        let running = crate::session_manager::RunningSession {
            session_id: session_id.clone(),
            agent_id: agent_id.unwrap_or_default(),
            agent_name: agent_name.clone(),
            input_tx: tokio::sync::mpsc::unbounded_channel().0,
            stream_sender,
            cancel_token,
            join_handle,
            chat_id: 0,
        };
        session_manager.register(running).await;

        info!(
            session_id = %session_id,
            agent = %agent_name,
            parent = ?self.session_id,
            "spawned child session directly"
        );

        Ok(ToolResult::success(format!(
            "Session {session_id} spawned for '{agent_name}'. Running asynchronously — result will be recorded when complete."
        )))
    }

    /// Handle delegation to a department channel.
    async fn delegate_to_department(
        &self,
        dept: &str,
        prompt: &str,
        response_mode: &str,
    ) -> Result<ToolResult> {
        // Send a DelegateRequest dispatch addressed to the department.
        // The trigger/routing system will pick it up and deliver to appropriate agents.
        let kind = DispatchKind::DelegateRequest {
            prompt: prompt.to_string(),
            response_mode: response_mode.to_string(),
            create_task: false,
            skill: None,
            reply_to: None,
            parent_session_id: self.session_id.clone(),
        };

        let to = format!("dept:{dept}");
        let dispatch = Dispatch::new_typed(&self.agent_name, &to, kind);
        let dispatch_id = dispatch.id.clone();

        info!(
            from = %self.agent_name,
            department = %dept,
            dispatch_id = %dispatch_id,
            "sending DelegateRequest to department"
        );

        self.dispatch_bus.send(dispatch).await;

        // Emit DepartmentMessage event for trigger system / observers.
        if let Some(ref broadcaster) = self.event_broadcaster {
            broadcaster.publish(ExecutionEvent::DepartmentMessage {
                department_id: dept.to_string(),
                department_name: dept.to_string(),
                from_agent: self.agent_name.clone(),
                content: prompt.to_string(),
            });
        }

        Ok(ToolResult::success(format!(
            "Delegation posted to department '{dept}' (dispatch_id: {dispatch_id}, response_mode: {response_mode})"
        )))
    }
}

#[async_trait]
impl Tool for DelegateTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let to = args
            .get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required parameter 'to'"))?;
        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required parameter 'prompt'"))?;

        let response_mode = Self::parse_response_mode(&args);
        let create_task = args
            .get("create_task")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let skill = args.get("skill").and_then(|v| v.as_str()).map(String::from);

        match to {
            // Pattern 1: Subagent — spawn session directly (or fallback to dispatch)
            "subagent" => {
                if self.provider.is_some() && self.session_manager.is_some() {
                    // Direct spawn — no dispatch bus, no patrol delay.
                    self.spawn_session(prompt).await
                } else {
                    // Fallback to dispatch (legacy path — provider/session_manager not wired).
                    let target = self.resolve_subagent_target().await;
                    let target = match target {
                        Some(name) => name,
                        None => {
                            return Ok(ToolResult::error(
                                "No target agent available for subagent delegation. \
                                 Configure a project-default agent or system escalation target.",
                            ));
                        }
                    };

                    info!(
                        from = %self.agent_name,
                        resolved_target = %target,
                        "subagent request routed to project-default agent (dispatch fallback)"
                    );

                    self.delegate_to_agent(&target, prompt, "origin", true, skill)
                        .await
                }
            }

            // Pattern 3: Department — post to department channel
            dept_target if dept_target.starts_with("dept:") => {
                let dept_name = &dept_target[5..]; // strip "dept:" prefix
                if dept_name.is_empty() {
                    return Ok(ToolResult::error(
                        "Department name cannot be empty. Use 'dept:<name>' format.",
                    ));
                }
                self.delegate_to_department(dept_name, prompt, &response_mode)
                    .await
            }

            // Pattern 2 & 4: Named agent (or fallback for unknown targets)
            agent_name => {
                // Self-delegation: spawn a child session instead of dispatching to yourself.
                if agent_name == self.agent_name
                    && self.provider.is_some()
                    && self.session_manager.is_some()
                {
                    self.spawn_session(prompt).await
                } else {
                    self.delegate_to_agent(agent_name, prompt, &response_mode, create_task, skill)
                        .await
                }
            }
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "aeqi_delegate".to_string(),
            description: "Delegate work to subagents, named agents, or departments. \
                Routes based on the 'to' parameter: \
                'subagent' spawns an ephemeral sub-agent, \
                'dept:<name>' posts to a department channel, \
                or any other value sends a delegation request to a named agent. \
                Response mode controls how results are returned: \
                'origin' (inject back into caller), \
                'perpetual' (deliver to perpetual session), \
                'async' (fire-and-forget with notification), \
                'department' (post to department channel), \
                'none' (no response expected)."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Target: 'subagent' for ephemeral agent, 'dept:<name>' for department, or an agent name"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "The task or message to delegate"
                    },
                    "response": {
                        "type": "string",
                        "enum": ["origin", "perpetual", "async", "department", "none"],
                        "default": "origin",
                        "description": "How the response should be routed back"
                    },
                    "create_task": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether to also create a tracked task for this delegation"
                    },
                    "skill": {
                        "type": "string",
                        "description": "Optional skill hint for the target agent"
                    },
                    "tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional tool allowlist for subagent mode"
                    }
                },
                "required": ["to", "prompt"]
            }),
        }
    }

    fn name(&self) -> &str {
        "aeqi_delegate"
    }

    fn is_concurrent_safe(&self, _input: &serde_json::Value) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool() -> DelegateTool {
        let bus = Arc::new(DispatchBus::new());
        DelegateTool::new(bus, "test-agent".to_string())
    }

    #[test]
    fn test_parse_response_mode_default() {
        let args = serde_json::json!({});
        assert_eq!(DelegateTool::parse_response_mode(&args), "origin");
    }

    #[test]
    fn test_parse_response_mode_explicit() {
        let args = serde_json::json!({"response": "async"});
        assert_eq!(DelegateTool::parse_response_mode(&args), "async");
    }

    #[test]
    fn test_spec_has_required_fields() {
        let tool = make_tool();
        let spec = tool.spec();
        assert_eq!(spec.name, "aeqi_delegate");
        let required = spec.input_schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("to")));
        assert!(required.contains(&serde_json::json!("prompt")));
    }

    #[test]
    fn test_name() {
        let tool = make_tool();
        assert_eq!(tool.name(), "aeqi_delegate");
    }

    #[tokio::test]
    async fn test_subagent_no_target_returns_error() {
        // Without agent_registry or fallback_target, subagent should error.
        let tool = make_tool();
        let args = serde_json::json!({
            "to": "subagent",
            "prompt": "do something"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("No target agent available"));
    }

    #[tokio::test]
    async fn test_subagent_with_fallback_target() {
        let bus = Arc::new(DispatchBus::new());
        let mut tool = DelegateTool::new(bus.clone(), "caller".to_string());
        tool.fallback_target = Some("leader".to_string());

        let args = serde_json::json!({
            "to": "subagent",
            "prompt": "handle this task",
            "skill": "code-review"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("leader"));
        assert!(result.output.contains("dispatch_id"));
        assert!(result.output.contains("Task creation requested"));
        assert!(result.output.contains("code-review"));

        // Verify the dispatch was sent to the fallback target.
        let messages = bus.read("leader").await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "caller");
        assert_eq!(messages[0].to, "leader");
        assert_eq!(messages[0].kind.subject_tag(), "DELEGATE_REQUEST");
    }

    #[tokio::test]
    async fn test_department_mode_detection() {
        let tool = make_tool();
        let args = serde_json::json!({
            "to": "dept:engineering",
            "prompt": "review this PR"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("engineering"));
        assert!(result.output.contains("dispatch_id"));
    }

    #[tokio::test]
    async fn test_department_empty_name_rejected() {
        let tool = make_tool();
        let args = serde_json::json!({
            "to": "dept:",
            "prompt": "review this PR"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_named_agent_dispatch() {
        let tool = make_tool();
        let args = serde_json::json!({
            "to": "researcher",
            "prompt": "find the auth bug",
            "response": "async",
            "create_task": true,
            "skill": "code-review"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("researcher"));
        assert!(result.output.contains("dispatch_id"));
        assert!(result.output.contains("Task creation requested"));
        assert!(result.output.contains("code-review"));
    }

    #[tokio::test]
    async fn test_missing_to_param() {
        let tool = make_tool();
        let args = serde_json::json!({
            "prompt": "do something"
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_prompt_param() {
        let tool = make_tool();
        let args = serde_json::json!({
            "to": "researcher"
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dispatch_actually_sent() {
        let bus = Arc::new(DispatchBus::new());
        let tool = DelegateTool::new(bus.clone(), "sender".to_string());

        let args = serde_json::json!({
            "to": "receiver",
            "prompt": "hello agent"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        // Verify the dispatch landed in the bus.
        let messages = bus.read("receiver").await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "sender");
        assert_eq!(messages[0].to, "receiver");
        assert_eq!(messages[0].kind.subject_tag(), "DELEGATE_REQUEST");
    }

    #[tokio::test]
    async fn test_department_dispatch_sent() {
        let bus = Arc::new(DispatchBus::new());
        let tool = DelegateTool::new(bus.clone(), "leader".to_string());

        let args = serde_json::json!({
            "to": "dept:ops",
            "prompt": "check server health"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        // Verify dispatch was sent to "dept:ops".
        let messages = bus.read("dept:ops").await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "leader");
        assert_eq!(messages[0].kind.subject_tag(), "DELEGATE_REQUEST");
    }
}
