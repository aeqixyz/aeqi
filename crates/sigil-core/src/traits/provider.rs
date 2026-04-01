use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// A single message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Convert to a searchable text representation including tool calls/results.
    pub fn to_transcript_text(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Parts(parts) => {
                let mut texts = Vec::new();
                for part in parts {
                    match part {
                        ContentPart::Text { text } => texts.push(text.clone()),
                        ContentPart::ToolUse { name, input, .. } => {
                            let input_str = serde_json::to_string(input).unwrap_or_default();
                            let preview: String = input_str.chars().take(500).collect();
                            texts.push(format!("[tool:{name}] {preview}"));
                        }
                        ContentPart::ToolResult {
                            content, is_error, ..
                        } => {
                            let prefix = if *is_error { "[error]" } else { "[result]" };
                            let preview: String = content.chars().take(500).collect();
                            texts.push(format!("{prefix} {preview}"));
                        }
                    }
                }
                texts.join("\n")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

/// Request to an LLM provider.
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for ChatRequest {
    fn default() -> Self {
        Self {
            model: String::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            max_tokens: 4096,
            temperature: 0.0,
        }
    }
}

/// Tool specification for the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A tool call parsed from the LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Response from an LLM provider.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Unknown(String),
}

/// Events emitted during streaming.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Incremental text content.
    TextDelta(String),
    /// A tool use block is starting.
    ToolUseStart { id: String, name: String },
    /// Incremental JSON input for the current tool use block.
    ToolUseInput(String),
    /// The complete response has been assembled (final usage available).
    MessageComplete(ChatResponse),
    /// Token usage update (may arrive mid-stream or at end).
    Usage(Usage),
}

/// LLM provider trait. All providers must implement this.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Send a chat request and get a response.
    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatResponse>;

    /// Stream a chat response as incremental events.
    ///
    /// Default implementation wraps `chat()` — providers override for true streaming.
    /// The sender is used to emit events; the final `MessageComplete` event MUST be sent.
    async fn chat_stream(
        &self,
        request: &ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let response = self.chat(request).await?;
        // Emit text delta if present.
        if let Some(ref text) = response.content {
            let _ = tx.send(StreamEvent::TextDelta(text.clone())).await;
        }
        // Emit tool use events.
        for tc in &response.tool_calls {
            let _ = tx
                .send(StreamEvent::ToolUseStart {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                })
                .await;
            let _ = tx
                .send(StreamEvent::ToolUseInput(tc.arguments.to_string()))
                .await;
        }
        // Emit usage and completion.
        let _ = tx.send(StreamEvent::Usage(response.usage.clone())).await;
        let _ = tx.send(StreamEvent::MessageComplete(response)).await;
        Ok(())
    }

    /// Provider name for logging/metrics.
    fn name(&self) -> &str;

    /// Check if the provider is healthy/reachable.
    async fn health_check(&self) -> anyhow::Result<()>;
}
