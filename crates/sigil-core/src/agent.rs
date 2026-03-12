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
    /// Entity ID for scoped memory queries. None = domain scope.
    pub entity_id: Option<String>,
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
            entity_id: None,
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
            let mq = if let Some(ref cid) = self.config.entity_id {
                MemoryQuery::new(prompt, 5).with_entity(cid.clone())
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
                        && let MessageContent::Text(t) = &mut msg.content
                    {
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
        let mut mid_loop_recalls = 0u32;
        const MAX_MID_LOOP_RECALLS: u32 = 2;

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
                assistant_parts.push(ContentPart::Text { text: text.clone() });
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

            // Execute tool calls in parallel for concurrency.
            let tool_futures: Vec<_> = response
                .tool_calls
                .iter()
                .map(|tc| {
                    let tools = self.tools.clone();
                    let name = tc.name.clone();
                    let args = tc.arguments.clone();
                    let id = tc.id.clone();
                    async move {
                        let start = std::time::Instant::now();
                        let result = Self::execute_tool_static(&tools, &name, args).await;
                        let duration_ms = start.elapsed().as_millis() as u64;
                        (id, name, result, duration_ms)
                    }
                })
                .collect();

            let results = futures::future::join_all(tool_futures).await;

            let mut tool_result_parts: Vec<ContentPart> = Vec::new();
            for (id, name, result, duration_ms) in results {
                match &result {
                    Ok(tr) => {
                        self.observer
                            .record(Event::ToolCall {
                                tool_name: name,
                                duration_ms,
                            })
                            .await;

                        tool_result_parts.push(ContentPart::ToolResult {
                            tool_use_id: id,
                            content: tr.output.clone(),
                            is_error: tr.is_error,
                        });
                    }
                    Err(e) => {
                        self.observer
                            .record(Event::ToolError {
                                tool_name: name,
                                error: e.to_string(),
                            })
                            .await;

                        tool_result_parts.push(ContentPart::ToolResult {
                            tool_use_id: id,
                            content: format!("Tool execution error: {e}"),
                            is_error: true,
                        });
                    }
                }
            }

            messages.push(Message {
                role: Role::Tool,
                content: MessageContent::Parts(tool_result_parts.clone()),
            });

            // Mid-loop memory recall: re-query memory if tool results surface
            // new domain terms not present in the original prompt.
            if mid_loop_recalls < MAX_MID_LOOP_RECALLS
                && let Some(ref mem) = self.memory
            {
                let tool_output: String = tool_result_parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::ToolResult {
                            content,
                            is_error: false,
                            ..
                        } => Some(content.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                if tool_output.len() > 200 && Self::has_novel_terms(&tool_output, prompt) {
                    let mq = MemoryQuery::new(&tool_output, 3).with_scope(MemoryScope::Domain);
                    if let Ok(entries) = mem.search(&mq).await
                        && !entries.is_empty()
                    {
                        let ctx = entries
                            .iter()
                            .map(|e| format!("[{}] {}: {}", e.scope, e.key, e.content))
                            .collect::<Vec<_>>()
                            .join("\n");
                        messages.push(Message {
                            role: Role::System,
                            content: MessageContent::text(format!(
                                "# Updated Memory Recall\n{ctx}"
                            )),
                        });
                        mid_loop_recalls += 1;
                        debug!(
                            agent = %self.config.name,
                            recall = mid_loop_recalls,
                            entries = entries.len(),
                            "mid-loop memory recall injected"
                        );
                    }
                }
            }

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
             - SYSTEM: Insights about the user (preferences, decisions, patterns that span projects)\n\
             - SELF: Your own observations, reflections, learnings as an agent\n\n\
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
            } else if let Some(r) = line.strip_prefix("SYSTEM ") {
                (MemoryScope::System, r)
            } else if let Some(r) = line.strip_prefix("SELF ") {
                (MemoryScope::Entity, r)
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

            let entity_id = if scope == MemoryScope::Entity {
                self.config.entity_id.as_deref()
            } else {
                None
            };

            match mem.store(key, content, category, scope, entity_id).await {
                Ok(id) => {
                    debug!(agent = %self.config.name, id = %id, key = %key, scope = %scope, "insight stored")
                }
                Err(e) => {
                    warn!(agent = %self.config.name, key = %key, "failed to store insight: {e}")
                }
            }
        }
    }

    /// Check if tool output contains novel terms not present in the original prompt.
    /// Returns true if there are 3+ words >5 chars that are new.
    fn has_novel_terms(tool_output: &str, original_prompt: &str) -> bool {
        let prompt_words: std::collections::HashSet<&str> = original_prompt
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| w.len() > 5)
            .collect();

        let novel_count = tool_output
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| w.len() > 5 && !prompt_words.contains(w))
            .take(3) // short-circuit after finding 3
            .count();

        novel_count >= 3
    }

    /// Find and execute a tool by name (static version for parallel execution).
    async fn execute_tool_static(
        tools: &[Arc<dyn Tool>],
        name: &str,
        args: serde_json::Value,
    ) -> anyhow::Result<ToolResult> {
        for tool in tools {
            if tool.name() == name {
                return tool.execute(args).await;
            }
        }
        Ok(ToolResult::error(format!("Unknown tool: {name}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_novel_terms_true() {
        let prompt = "Fix the authentication bug in login.rs";
        let output = "Found DatabaseConnection, TransactionPool, and ConnectionManager \
                       types in the codebase that handle connection lifecycle";
        assert!(Agent::has_novel_terms(output, prompt));
    }

    #[test]
    fn test_has_novel_terms_false_same_words() {
        let prompt = "Fix authentication and login handling";
        let output = "Checked authentication and login handling";
        assert!(!Agent::has_novel_terms(output, prompt));
    }

    #[test]
    fn test_has_novel_terms_false_short_output() {
        let prompt = "Fix the bug";
        let output = "ok done";
        assert!(!Agent::has_novel_terms(output, prompt));
    }
}
