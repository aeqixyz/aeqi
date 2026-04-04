//! Session Manager — holds running agent sessions in memory.
//!
//! Each running session is a spawned `Agent::run()` task with a perpetual input
//! channel. Messages are injected via `input_tx`, responses collected via
//! `ChatStreamSender` broadcast. Sessions persist until explicitly closed (which
//! drops the input channel, causing the agent loop to exit).
//!
//! Two kinds of sessions:
//! - **Permanent**: one per agent, always alive, IS the agent's identity
//! - **Spawned**: created by triggers, skills, or users — persistent until closed

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, info, warn};

use aeqi_core::AgentResult;
use aeqi_core::chat_stream::{ChatStreamEvent, ChatStreamSender};
use aeqi_core::traits::{Memory, Provider};

use crate::agent_registry::AgentRegistry;
use crate::execution_events::EventBroadcaster;
use crate::message::DispatchBus;
use crate::notes::Notes;
use crate::registry::CompanyRegistry;
use crate::session_store::SessionStore;

/// A running agent session — the in-memory handle to a live agent loop.
pub struct RunningSession {
    pub session_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub input_tx: mpsc::UnboundedSender<String>,
    pub stream_sender: ChatStreamSender,
    pub cancel_token: Arc<std::sync::atomic::AtomicBool>,
    pub join_handle: tokio::task::JoinHandle<anyhow::Result<AgentResult>>,
    pub chat_id: i64,
}

impl RunningSession {
    /// Send a message and wait for the agent's response.
    ///
    /// Subscribes to the stream, pushes the message, collects TextDelta events
    /// until a Complete event arrives. Returns the accumulated response text
    /// and token counts.
    pub async fn send_and_wait(&self, message: &str) -> anyhow::Result<SessionResponse> {
        // Subscribe BEFORE pushing so we don't miss events.
        let mut rx = self.stream_sender.subscribe();

        // Push message into the agent loop.
        self.input_tx
            .send(message.to_string())
            .map_err(|_| anyhow::anyhow!("session closed — agent loop exited"))?;

        // Collect response.
        let mut text = String::new();
        let mut iterations = 0u32;
        let mut prompt_tokens = 0u32;
        let mut completion_tokens = 0u32;

        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(300), rx.recv()).await {
                Ok(Ok(event)) => match event {
                    ChatStreamEvent::TextDelta { text: delta } => {
                        text.push_str(&delta);
                    }
                    ChatStreamEvent::Complete {
                        total_prompt_tokens,
                        total_completion_tokens,
                        iterations: iters,
                        ..
                    } => {
                        prompt_tokens = total_prompt_tokens;
                        completion_tokens = total_completion_tokens;
                        iterations = iters;
                        break;
                    }
                    _ => {
                        // TurnStart, ToolStart, ToolComplete, etc. — skip.
                    }
                },
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                    warn!(lagged = n, "stream subscriber lagged — some events lost");
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                    // Agent loop ended without a Complete event.
                    break;
                }
                Err(_) => {
                    return Err(anyhow::anyhow!("session response timed out (300s)"));
                }
            }
        }

        Ok(SessionResponse {
            text,
            iterations,
            prompt_tokens,
            completion_tokens,
        })
    }

    /// Check if the agent loop is still running.
    pub fn is_alive(&self) -> bool {
        !self.join_handle.is_finished()
    }
}

/// Response from a session send.
pub struct SessionResponse {
    pub text: String,
    pub iterations: u32,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// What kind of session to spawn.
pub enum SpawnType {
    /// Interactive perpetual session (web chat) — accepts follow-up messages.
    Interactive,
    /// Task execution — runs to completion, updates task.
    Task { task_id: String },
    /// Delegation — child of another session.
    Delegation { parent_id: String },
}

/// Returned from `spawn_session` — the caller uses this to subscribe to events.
pub struct SpawnedSession {
    pub session_id: String,
    pub stream_sender: ChatStreamSender,
}

/// Manages all running agent sessions in the daemon.
pub struct SessionManager {
    sessions: Mutex<HashMap<String, RunningSession>>,
    // Dependencies for spawn_session (injected via configure()).
    agent_registry: Option<Arc<AgentRegistry>>,
    session_store: Option<Arc<SessionStore>>,
    registry: Option<Arc<CompanyRegistry>>,
    default_model: String,
    event_broadcaster: Option<Arc<EventBroadcaster>>,
    dispatch_bus: Option<Arc<DispatchBus>>,
    notes: Option<Arc<Notes>>,
    shared_primer: Option<String>,
    project_primer: Option<String>,
    memory_stores: HashMap<String, Arc<dyn Memory>>,
    memory_stores_by_id: HashMap<String, Arc<dyn Memory>>,
    default_project: String,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            agent_registry: None,
            session_store: None,
            registry: None,
            default_model: String::new(),
            event_broadcaster: None,
            dispatch_bus: None,
            notes: None,
            shared_primer: None,
            project_primer: None,
            memory_stores: HashMap::new(),
            memory_stores_by_id: HashMap::new(),
            default_project: String::new(),
        }
    }

    /// Inject dependencies that aren't available at construction time.
    #[allow(clippy::too_many_arguments)]
    pub fn configure(
        &mut self,
        agent_registry: Arc<AgentRegistry>,
        session_store: Arc<SessionStore>,
        registry: Arc<CompanyRegistry>,
        default_model: String,
        event_broadcaster: Option<Arc<EventBroadcaster>>,
        dispatch_bus: Arc<DispatchBus>,
        notes: Option<Arc<Notes>>,
        memory_stores: HashMap<String, Arc<dyn Memory>>,
        memory_stores_by_id: HashMap<String, Arc<dyn Memory>>,
        default_project: String,
    ) {
        self.shared_primer = registry.shared_primer.clone();
        self.project_primer = registry.project_primer.clone();
        self.agent_registry = Some(agent_registry);
        self.session_store = Some(session_store);
        self.registry = Some(registry);
        self.default_model = default_model;
        self.event_broadcaster = event_broadcaster;
        self.dispatch_bus = Some(dispatch_bus);
        self.notes = notes;
        self.memory_stores = memory_stores;
        self.memory_stores_by_id = memory_stores_by_id;
        self.default_project = default_project;
    }

    /// Spawn a new agent session — the universal executor.
    ///
    /// Resolves agent, builds identity + tools, creates DB session, spawns
    /// the agent loop as a background task, and registers the running session.
    pub async fn spawn_session(
        &self,
        agent_id_or_hint: &str,
        prompt: &str,
        spawn_type: SpawnType,
        provider: Arc<dyn Provider>,
        project_id: Option<&str>,
    ) -> anyhow::Result<SpawnedSession> {
        let agent_registry = self
            .agent_registry
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session manager not configured (no agent_registry)"))?;
        let registry = self
            .registry
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session manager not configured (no registry)"))?;
        let dispatch_bus = self
            .dispatch_bus
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session manager not configured (no dispatch_bus)"))?;

        // 1. Resolve agent from agent_registry by UUID (or by name via resolve_by_hint).
        let agent_opt = if uuid::Uuid::parse_str(agent_id_or_hint).is_ok() {
            agent_registry.get(agent_id_or_hint).await.ok().flatten()
        } else {
            agent_registry
                .resolve_by_hint(agent_id_or_hint)
                .await
                .ok()
                .flatten()
        };

        let (
            agent_name,
            agent_system_prompt,
            agent_uuid,
            agent_company,
            agent_project_id,
            agent_department_id,
        ) = match agent_opt {
            Some(agent) => (
                agent.name.clone(),
                if agent.system_prompt.is_empty() {
                    "You are a helpful AI agent.".to_string()
                } else {
                    agent.system_prompt.clone()
                },
                Some(agent.id.clone()),
                agent.project.clone(),
                agent.project_id.clone(),
                agent.department_id.clone(),
            ),
            None => (
                agent_id_or_hint.to_string(),
                "You are a helpful AI agent.".to_string(),
                None,
                None,
                None,
                None,
            ),
        };

        // Use explicit project_id parameter if provided, falling back to agent's project_id.
        let effective_project_id = project_id
            .map(|s| s.to_string())
            .or(agent_project_id.clone());

        // 2. Build Identity — agent's system_prompt + shared/project primers.
        let mut knowledge_parts: Vec<String> = Vec::new();
        if let Some(ref sp) = self.shared_primer {
            knowledge_parts.push(sp.clone());
        }
        if let Some(ref pp) = self.project_primer {
            knowledge_parts.push(pp.clone());
        }
        let identity = aeqi_core::Identity {
            persona: Some(agent_system_prompt),
            knowledge: if knowledge_parts.is_empty() {
                None
            } else {
                Some(knowledge_parts.join("\n\n---\n\n"))
            },
            ..Default::default()
        };

        // 3. Resolve workdir from agent's project via CompanyRegistry.
        let workdir = {
            let project_name = agent_company
                .as_deref()
                .or(if self.default_project.is_empty() {
                    None
                } else {
                    Some(self.default_project.as_str())
                });
            if let Some(name) = project_name {
                if let Some(company) = registry.get_project(name).await {
                    company.repo.clone()
                } else {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"))
                }
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"))
            }
        };

        // 4. Build tools.
        let mut tools: Vec<Arc<dyn aeqi_core::traits::Tool>> = vec![
            Arc::new(aeqi_tools::ShellTool::new(workdir.clone())),
            Arc::new(aeqi_tools::FileReadTool::new(workdir.clone())),
            Arc::new(aeqi_tools::FileWriteTool::new(workdir.clone())),
            Arc::new(aeqi_tools::FileEditTool::new(workdir.clone())),
            Arc::new(aeqi_tools::GrepTool::new(workdir.clone())),
            Arc::new(aeqi_tools::GlobTool::new(workdir)),
            Arc::new(aeqi_tools::WebFetchTool),
            Arc::new(aeqi_tools::WebSearchTool),
        ];

        // 5. Resolve memory.
        let memory_for_agent: Option<Arc<dyn Memory>> = effective_project_id
            .as_deref()
            .and_then(|id| self.memory_stores_by_id.get(id))
            .or_else(|| {
                agent_company
                    .as_deref()
                    .and_then(|c| self.memory_stores.get(c))
            })
            .or_else(|| self.memory_stores.get(agent_id_or_hint))
            .or_else(|| {
                if !self.default_project.is_empty() {
                    self.memory_stores.get(&self.default_project)
                } else {
                    self.memory_stores.values().next()
                }
            })
            .cloned();

        // Resolve graph DB path.
        let graph_company = agent_company
            .as_deref()
            .or(if self.default_project.is_empty() {
                None
            } else {
                Some(self.default_project.as_str())
            });
        let graph_db_path = graph_company.and_then(|c| {
            let data_dir = std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".aeqi"))
                .unwrap_or_else(|_| PathBuf::from("/tmp"));
            let path = data_dir.join("codegraph").join(format!("{c}.db"));
            path.exists().then_some(path)
        });

        // Determine session_id placeholder for delegate tool wiring (filled in after DB create).
        let is_interactive = matches!(spawn_type, SpawnType::Interactive);

        // Build orchestration tools (delegate, memory, notes, graph, etc.)
        let empty_channels: Arc<
            tokio::sync::RwLock<
                std::collections::HashMap<String, Arc<dyn aeqi_core::traits::Channel>>,
            >,
        > = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

        let orch_tools = crate::tools::build_orchestration_tools(
            registry.clone(),
            dispatch_bus.clone(),
            empty_channels,
            None,
            memory_for_agent.clone(),
            self.notes.clone(),
            self.event_broadcaster.clone(),
            graph_db_path,
            None, // session_id — not yet known, delegate uses parent_session_id from SpawnType
            Some(provider.clone()),
            self.session_store.clone(),
            Some(Arc::new(Self::new())), // placeholder — delegate spawns go through dispatch
            self.default_model.clone(),
        );
        tools.extend(orch_tools);

        if let Some(ref ss) = self.session_store {
            tools.push(Arc::new(crate::tools::TranscriptSearchTool::new(
                ss.clone(),
            )));
        }

        // 6. Build AgentConfig — Perpetual for Interactive, Async for others.
        let context_window = aeqi_providers::context_window_for_model(&self.default_model);
        let session_type = if is_interactive {
            aeqi_core::SessionType::Perpetual
        } else {
            aeqi_core::SessionType::Async
        };
        let max_iterations = if is_interactive { 200 } else { 50 };

        let agent_config = aeqi_core::AgentConfig {
            model: self.default_model.clone(),
            max_iterations,
            name: agent_name.clone(),
            context_window,
            entity_id: agent_uuid.clone(),
            department_id: agent_department_id.clone(),
            project_id: effective_project_id.clone(),
            session_type,
            ..Default::default()
        };

        // 7. Create Agent with ChatStreamSender, attach memory.
        let observer: Arc<dyn aeqi_core::traits::Observer> =
            Arc::new(aeqi_core::traits::LogObserver);

        let (stream_sender, _initial_rx) = ChatStreamSender::new(256);

        let mut agent = aeqi_core::Agent::new(agent_config, provider, tools, observer, identity)
            .with_chat_stream(stream_sender.clone());

        if let Some(ref mem) = memory_for_agent {
            agent = agent.with_memory(mem.clone());
        }

        // 8. If Interactive, create perpetual input channel.
        let (agent, input_tx, cancel_token) = if is_interactive {
            let cancel = agent.cancel_token();
            let (agent, tx) = agent.with_perpetual_input();
            (agent, tx, cancel)
        } else {
            let cancel = agent.cancel_token();
            let (tx, _rx) = mpsc::unbounded_channel();
            (agent, tx, cancel)
        };

        // 9. Create session in DB.
        let (parent_id, task_id) = match &spawn_type {
            SpawnType::Interactive => (None, None),
            SpawnType::Task { task_id } => (None, Some(task_id.as_str())),
            SpawnType::Delegation { parent_id } => (Some(parent_id.as_str()), None),
        };
        let session_type_str = match &spawn_type {
            SpawnType::Interactive => "perpetual",
            SpawnType::Task { .. } => "task",
            SpawnType::Delegation { .. } => "delegation",
        };

        let session_id = if let Some(ref ss) = self.session_store {
            let aid = agent_uuid.as_deref().unwrap_or("");
            ss.create_session(
                aid,
                effective_project_id.as_deref(),
                agent_department_id.as_deref(),
                session_type_str,
                &agent_name,
                parent_id,
                task_id,
            )
            .await
            .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string())
        } else {
            uuid::Uuid::new_v4().to_string()
        };

        // 10. Record prompt as user message.
        if let Some(ref ss) = self.session_store {
            let _ = ss
                .record_by_session(&session_id, "user", prompt, Some(session_type_str))
                .await;
        }

        // 11. Spawn via tokio::spawn.
        let prompt_owned = prompt.to_string();
        let ss_clone = self.session_store.clone();
        let sid_clone = session_id.clone();
        let is_interactive_spawn = is_interactive;

        let join_handle = tokio::spawn(async move {
            let result = agent.run(&prompt_owned).await;
            // On completion, record result and close session (unless Interactive — those
            // stay open until explicitly closed).
            if !is_interactive_spawn && let (Some(ss), Ok(r)) = (&ss_clone, &result) {
                let _ = ss
                    .record_by_session(&sid_clone, "assistant", &r.text, Some("session"))
                    .await;
                let _ = ss.close_session(&sid_clone).await;
            }
            result
        });

        // 12. Register RunningSession.
        let running = RunningSession {
            session_id: session_id.clone(),
            agent_id: agent_uuid.unwrap_or_default(),
            agent_name: agent_name.clone(),
            input_tx,
            stream_sender: stream_sender.clone(),
            cancel_token,
            join_handle,
            chat_id: 0,
        };
        self.register(running).await;

        info!(
            session_id = %session_id,
            agent = %agent_name,
            spawn_type = session_type_str,
            "spawn_session: session spawned"
        );

        // 13. Return SpawnedSession.
        Ok(SpawnedSession {
            session_id,
            stream_sender,
        })
    }

    /// Register a running session.
    pub async fn register(&self, session: RunningSession) {
        let session_id = session.session_id.clone();
        let agent_name = session.agent_name.clone();
        info!(session_id = %session_id, agent = %agent_name, "session registered");
        self.sessions.lock().await.insert(session_id, session);
    }

    /// Get a reference to a running session for sending messages.
    /// Returns None if session doesn't exist or agent loop has exited.
    pub async fn get(&self, session_id: &str) -> Option<()> {
        let sessions = self.sessions.lock().await;
        sessions
            .get(session_id)
            .and_then(|s| if s.is_alive() { Some(()) } else { None })
    }

    /// Send a message to a running session and wait for the response.
    pub async fn send(&self, session_id: &str, message: &str) -> anyhow::Result<SessionResponse> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session '{}' not running", session_id))?;

        if !session.is_alive() {
            return Err(anyhow::anyhow!(
                "session '{}' agent loop has exited",
                session_id
            ));
        }

        session.send_and_wait(message).await
    }

    /// Subscribe to a session's stream for real-time events.
    pub async fn subscribe(
        &self,
        session_id: &str,
    ) -> Option<tokio::sync::broadcast::Receiver<ChatStreamEvent>> {
        let sessions = self.sessions.lock().await;
        sessions
            .get(session_id)
            .map(|s| s.stream_sender.subscribe())
    }

    /// Inject a message into a running session without waiting for the response.
    /// Returns a broadcast receiver for streaming events. The caller reads events
    /// from the receiver until Complete arrives.
    pub async fn send_streaming(
        &self,
        session_id: &str,
        message: &str,
    ) -> anyhow::Result<tokio::sync::broadcast::Receiver<ChatStreamEvent>> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session '{}' not running", session_id))?;

        if !session.is_alive() {
            return Err(anyhow::anyhow!(
                "session '{}' agent loop has exited",
                session_id
            ));
        }

        // Subscribe BEFORE pushing so we don't miss events.
        let rx = session.stream_sender.subscribe();

        session
            .input_tx
            .send(message.to_string())
            .map_err(|_| anyhow::anyhow!("session closed — agent loop exited"))?;

        Ok(rx)
    }

    /// Remove and shut down a session. Drops input_tx which causes the agent
    /// loop to exit at the next await point.
    pub async fn close(&self, session_id: &str) -> bool {
        let removed = self.sessions.lock().await.remove(session_id);
        if let Some(session) = removed {
            info!(
                session_id = %session_id,
                agent = %session.agent_name,
                "session closed — dropping input channel"
            );
            // Drop input_tx — agent loop sees None from recv() and exits.
            drop(session.input_tx);
            // Cancel token as backup.
            session
                .cancel_token
                .store(true, std::sync::atomic::Ordering::Relaxed);
            true
        } else {
            debug!(session_id = %session_id, "close: session not found (already stopped?)");
            false
        }
    }

    /// Reap dead sessions (agent loops that exited on their own).
    pub async fn reap_dead(&self) {
        let mut sessions = self.sessions.lock().await;
        let dead: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| !s.is_alive())
            .map(|(id, _)| id.clone())
            .collect();

        for id in &dead {
            sessions.remove(id);
        }

        if !dead.is_empty() {
            info!(count = dead.len(), "reaped dead sessions: {:?}", dead);
        }
    }

    /// List all running session IDs.
    pub async fn list_running(&self) -> Vec<String> {
        self.sessions.lock().await.keys().cloned().collect()
    }

    /// Check if a session is running.
    pub async fn is_running(&self, session_id: &str) -> bool {
        let sessions = self.sessions.lock().await;
        sessions.get(session_id).is_some_and(|s| s.is_alive())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
