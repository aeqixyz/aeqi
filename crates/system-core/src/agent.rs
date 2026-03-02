use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::identity::Identity;
use crate::traits::{
    ChatRequest, ChatResponse, ContentPart, Event, Memory, MemoryCategory, MemoryQuery,
    MemoryScope, Message, MessageContent, Observer, Provider, Role, StopReason, Tool, ToolResult,
    ToolSpec,
};

/// Configuration for an agent loop.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Model to use (provider-specific format).
    pub model: String,
    /// Maximum iterations (LLM round-trips) before stopping.
    pub max_iterations: u32,
    /// Maximum tokens per LLM response.
    pub max_tokens: u32,
    /// Temperature for generation.
    pub temperature: f32,
    /// Name of this agent (for logging).
    pub name: String,
    /// Companion ID for scoped memory queries. None = domain scope.
    pub companion_id: Option<String>,
}

/// Result from an agent run, including token usage for cost tracking.
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub text: String,
    pub total_prompt_tokens: u32,
    pub total_completion_tokens: u32,
    pub iterations: u32,
    pub model: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "anthropic/claude-sonnet-4.6".to_string(),
            max_iterations: 20,
            max_tokens: 4096,
            temperature: 0.0,
            name: "agent".to_string(),
            companion_id: None,
        }
    }
}

/// The agent: a thin loop that sends prompts to an LLM, parses tool calls,
/// executes tools, and repeats until done. Zero Framework Cognition — no
/// heuristics, all decisions delegated to the LLM.
pub struct Agent {
    config: AgentConfig,
    provider: Arc<dyn Provider>,
    tools: Vec<Arc<dyn Tool>>,
    observer: Arc<dyn Observer>,
    identity: Identity,
    memory: Option<Arc<dyn Memory>>,
}

impl Agent {
    pub fn new(
        config: AgentConfig,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        observer: Arc<dyn Observer>,
        identity: Identity,
    ) -> Self {
        Self {
            config,
            provider,
            tools,
            observer,
            identity,
            memory: None,
        }
    }

    /// Attach a memory backend for context recall.
    pub fn with_memory(mut self, memory: Arc<dyn Memory>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Run the agent with a user prompt. Returns the final text, token usage, and iterations.
    pub async fn run(&self, prompt: &str) -> anyhow::Result<AgentResult> {
        self.observer
            .record(Event::AgentStart {
                agent_name: self.config.name.clone(),
            })
            .await;

        // Build initial messages.
        let system_prompt = self.identity.system_prompt();
        let mut messages = vec![
            Message {
                role: Role::System,
                content: MessageContent::text(&system_prompt),
            },
            Message {
                role: Role::User,
                content: MessageContent::text(prompt),
            },
        ];

        // Inject recalled memory context into system prompt.
        if let Some(ref mem) = self.memory {
            let mq = if let Some(ref cid) = self.config.companion_id {
                MemoryQuery::new(prompt, 5).with_companion(cid.clone())
            } else {
                MemoryQuery::new(prompt, 5).with_scope(MemoryScope::Domain)
            };

            match mem.search(&mq).await {
                Ok(entries) if !entries.is_empty() => {
                    let ctx = entries
                        .iter()
                        .map(|e| format!("[{}] {}: {}", e.scope, e.key, e.content))
                        .collect::<Vec<_>>()
                        .join("\n");

                    if let Some(msg) = messages.first_mut()
                        && let MessageContent::Text(t) = &mut msg.content {
                            *t = format!("{t}\n\n# Recalled Memory\n{ctx}");
                        }

                    debug!(agent = %self.config.name, count = entries.len(), "memory context injected");
                }
                Ok(_) => {}
                Err(e) => warn!(agent = %self.config.name, "memory recall failed: {e}"),
            }
        }

        // Collect tool specs.
        let tool_specs: Vec<ToolSpec> = self.tools.iter().map(|t| t.spec()).collect();

        let mut iterations = 0u32;
        let mut final_text = String::new();
        let mut total_prompt_tokens = 0u32;
        let mut total_completion_tokens = 0u32;

        loop {
            iterations += 1;
            if iterations > self.config.max_iterations {
                warn!(
                    agent = %self.config.name,
                    max = self.config.max_iterations,
                    "agent hit max iterations"
                );
                break;
            }

            // Build request.
            let request = ChatRequest {
                model: self.config.model.clone(),
                messages: messages.clone(),
                tools: tool_specs.clone(),
                max_tokens: self.config.max_tokens,
                temperature: self.config.temperature,
            };

            self.observer
                .record(Event::LlmRequest {
                    model: self.config.model.clone(),
                    tokens: 0, // We don't know prompt tokens until response.
                })
                .await;

            // Call provider.
            let response: ChatResponse = self.provider.chat(&request).await?;

            total_prompt_tokens += response.usage.prompt_tokens;
            total_completion_tokens += response.usage.completion_tokens;

            self.observer
                .record(Event::LlmResponse {
                    model: self.config.model.clone(),
                    prompt_tokens: response.usage.prompt_tokens,
                    completion_tokens: response.usage.completion_tokens,
                })
                .await;

            debug!(
                agent = %self.config.name,
                iteration = iterations,
                tool_calls = response.tool_calls.len(),
                stop_reason = ?response.stop_reason,
                "LLM response received"
            );

            // If there's text content, accumulate it.
            if let Some(ref text) = response.content {
                final_text = text.clone();
            }

            // If no tool calls, we're done.
            if response.tool_calls.is_empty() {
                break;
            }

            // Build assistant message with tool use parts.
            let mut assistant_parts: Vec<ContentPart> = Vec::new();
            if let Some(ref text) = response.content {
                assistant_parts.push(ContentPart::Text {
                    text: text.clone(),
                });
            }
            for tc in &response.tool_calls {
                assistant_parts.push(ContentPart::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                });
            }

            messages.push(Message {
                role: Role::Assistant,
                content: MessageContent::Parts(assistant_parts),
            });

            // Execute each tool call and build tool result messages.
            let mut tool_result_parts: Vec<ContentPart> = Vec::new();
            for tc in &response.tool_calls {
                let start = std::time::Instant::now();

                let result = self.execute_tool(&tc.name, tc.arguments.clone()).await;
                let duration_ms = start.elapsed().as_millis() as u64;

                match &result {
                    Ok(tr) => {
                        self.observer
                            .record(Event::ToolCall {
                                tool_name: tc.name.clone(),
                                duration_ms,
                            })
                            .await;

                        tool_result_parts.push(ContentPart::ToolResult {
                            tool_use_id: tc.id.clone(),
                            content: tr.output.clone(),
                            is_error: tr.is_error,
                        });
                    }
                    Err(e) => {
                        self.observer
                            .record(Event::ToolError {
                                tool_name: tc.name.clone(),
                                error: e.to_string(),
                            })
                            .await;

                        tool_result_parts.push(ContentPart::ToolResult {
                            tool_use_id: tc.id.clone(),
                            content: format!("Tool execution error: {e}"),
                            is_error: true,
                        });
                    }
                }
            }

            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Parts(tool_result_parts),
            });

            // If stop reason is EndTurn (not ToolUse), break.
            if response.stop_reason == StopReason::EndTurn {
                break;
            }
        }

        self.reflect(&messages).await;

        self.observer
            .record(Event::AgentEnd {
                agent_name: self.config.name.clone(),
                iterations,
            })
            .await;

        info!(
            agent = %self.config.name,
            iterations,
            prompt_tokens = total_prompt_tokens,
            completion_tokens = total_completion_tokens,
            "agent completed"
        );

        Ok(AgentResult {
            text: final_text,
            total_prompt_tokens,
            total_completion_tokens,
            iterations,
            model: self.config.model.clone(),
        })
    }

    async fn reflect(&self, messages: &[Message]) {
        let Some(ref mem) = self.memory else { return };

        let transcript = self.compact_transcript(messages);
        if transcript.len() < 50 {
            return;
        }

        let reflection_prompt = format!(
            "You are a memory extraction system. Analyze this conversation and extract ONLY \
             genuinely important insights worth remembering long-term. Output NOTHING if the \
             conversation is trivial (greetings, status checks, small talk).\n\n\
             For each insight, output exactly one line in this format:\n\
             SCOPE CATEGORY: key-slug | The insight content\n\n\
             Scopes (choose the most appropriate):\n\
             - DOMAIN: Technical facts about this specific project/codebase\n\
             - REALM: Insights about the Emperor (preferences, decisions, patterns that span domains)\n\
             - SELF: Your own observations, reflections, learnings as a companion\n\n\
             Categories:\n\
             - FACT: Factual information (technical details, architecture decisions, numbers)\n\
             - PROCEDURE: How something works or should be done\n\
             - PREFERENCE: User preferences, opinions, behavioral patterns\n\
             - CONTEXT: Decisions made, strategic shifts, project state changes\n\n\
             Rules:\n\
             - Maximum 5 insights per conversation\n\
             - Each insight must be self-contained (understandable without the conversation)\n\
             - key-slug: 2-4 lowercase hyphenated words\n\
             - Content: one concise sentence\n\
             - If nothing is worth remembering, output exactly: NONE\n\n\
             ## Conversation\n\n{}",
            transcript
        );

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text(&reflection_prompt),
            }],
            tools: vec![],
            max_tokens: 512,
            temperature: 0.0,
        };

        match self.provider.chat(&request).await {
            Ok(response) => {
                if let Some(text) = response.content {
                    self.store_insights(&text, mem).await;
                }
            }
            Err(e) => warn!(agent = %self.config.name, "reflection failed: {e}"),
        }
    }

    fn compact_transcript(&self, messages: &[Message]) -> String {
        let mut transcript = String::new();
        let max_len = 8000;

        for msg in messages {
            if transcript.len() >= max_len {
                break;
            }

            let role = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System | Role::Tool => continue,
            };

            let text = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Parts(parts) => parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            };

            if !text.is_empty() {
                let remaining = max_len.saturating_sub(transcript.len());
                if text.len() > remaining {
                    transcript.push_str(&format!("{role}: {}\n\n", &text[..remaining]));
                } else {
                    transcript.push_str(&format!("{role}: {text}\n\n"));
                }
            }
        }
        transcript
    }

    async fn store_insights(&self, text: &str, mem: &Arc<dyn Memory>) {
        for line in text.lines() {
            let line = line.trim();
            if line == "NONE" || line.is_empty() {
                continue;
            }

            let (scope, rest) = if let Some(r) = line.strip_prefix("DOMAIN ") {
                (MemoryScope::Domain, r)
            } else if let Some(r) = line.strip_prefix("REALM ") {
                (MemoryScope::Realm, r)
            } else if let Some(r) = line.strip_prefix("SELF ") {
                (MemoryScope::Companion, r)
            } else if let Some((cat_str, _)) = line.split_once(':') {
                let cat_str = cat_str.trim();
                if matches!(cat_str, "FACT" | "PROCEDURE" | "PREFERENCE" | "CONTEXT") {
                    (MemoryScope::Domain, line)
                } else {
                    continue;
                }
            } else {
                continue;
            };

            let Some((cat_str, rest)) = rest.split_once(':') else {
                continue;
            };
            let Some((key, content)) = rest.split_once('|') else {
                continue;
            };

            let category = match cat_str.trim().to_uppercase().as_str() {
                "FACT" => MemoryCategory::Fact,
                "PROCEDURE" => MemoryCategory::Procedure,
                "PREFERENCE" => MemoryCategory::Preference,
                "CONTEXT" => MemoryCategory::Context,
                _ => continue,
            };

            let key = key.trim();
            let content = content.trim();
            if key.is_empty() || content.is_empty() {
                continue;
            }

            let companion_id = if scope == MemoryScope::Companion {
                self.config.companion_id.as_deref()
            } else {
                None
            };

            match mem.store(key, content, category, scope, companion_id).await {
                Ok(id) => {
                    debug!(agent = %self.config.name, id = %id, key = %key, scope = %scope, "insight stored")
                }
                Err(e) => {
                    warn!(agent = %self.config.name, key = %key, "failed to store insight: {e}")
                }
            }
        }
    }

    /// Find and execute a tool by name.
    async fn execute_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> anyhow::Result<ToolResult> {
        for tool in &self.tools {
            if tool.name() == name {
                return tool.execute(args).await;
            }
        }
        Ok(ToolResult::error(format!("Unknown tool: {name}")))
    }
}
