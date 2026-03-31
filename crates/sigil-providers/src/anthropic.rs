use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sigil_core::traits::{
    ChatRequest, ChatResponse, ContentPart, Message, MessageContent, Provider, Role, StopReason,
    StreamEvent, ToolCall, ToolSpec, Usage,
};
use tokio::sync::mpsc;
use tracing::debug;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

/// Direct Anthropic API provider (Messages API).
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    default_model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, default_model: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            api_key,
            default_model,
        }
    }
}

// --- Anthropic API types ---

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: serde_json::Value,
}

#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    usage: AnthropicUsage,
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum AnthropicContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Deserialize)]
struct AnthropicError {
    error: AnthropicErrorDetail,
}

#[derive(Deserialize)]
struct AnthropicErrorDetail {
    message: String,
}

fn convert_messages(messages: &[Message]) -> (Option<String>, Vec<AnthropicMessage>) {
    let mut system = None;
    let mut converted = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System => {
                if let Some(text) = msg.content.as_text() {
                    system = Some(text.to_string());
                }
            }
            Role::User => {
                if let Some(text) = msg.content.as_text() {
                    converted.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: serde_json::Value::String(text.to_string()),
                    });
                }
            }
            Role::Assistant => match &msg.content {
                MessageContent::Text(text) => {
                    converted.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: serde_json::Value::String(text.clone()),
                    });
                }
                MessageContent::Parts(parts) => {
                    let mut content_blocks = Vec::new();
                    for part in parts {
                        match part {
                            ContentPart::Text { text } => {
                                content_blocks.push(serde_json::json!({
                                    "type": "text",
                                    "text": text,
                                }));
                            }
                            ContentPart::ToolUse { id, name, input } => {
                                content_blocks.push(serde_json::json!({
                                    "type": "tool_use",
                                    "id": id,
                                    "name": name,
                                    "input": input,
                                }));
                            }
                            _ => {}
                        }
                    }
                    converted.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: serde_json::Value::Array(content_blocks),
                    });
                }
            },
            Role::Tool => {
                if let MessageContent::Parts(parts) = &msg.content {
                    let mut content_blocks = Vec::new();
                    for part in parts {
                        if let ContentPart::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } = part
                        {
                            content_blocks.push(serde_json::json!({
                                "type": "tool_result",
                                "tool_use_id": tool_use_id,
                                "content": content,
                                "is_error": is_error,
                            }));
                        }
                    }
                    converted.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: serde_json::Value::Array(content_blocks),
                    });
                }
            }
        }
    }

    (system, converted)
}

fn convert_tools(tools: &[ToolSpec]) -> Vec<AnthropicTool> {
    tools
        .iter()
        .map(|t| AnthropicTool {
            name: t.name.clone(),
            description: t.description.clone(),
            input_schema: t.input_schema.clone(),
        })
        .collect()
}

// --- SSE streaming event types ---

/// Wrapper for parsing the top-level `type` field of each SSE event.
#[derive(Deserialize)]
struct SseEvent {
    #[serde(rename = "type")]
    event_type: String,
    // Remaining fields are parsed separately per event type.
}

#[derive(Deserialize)]
struct SseMessageStart {
    message: SseMessageStartMessage,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct SseMessageStartMessage {
    model: Option<String>,
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct SseContentBlockStart {
    index: usize,
    content_block: SseContentBlock,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum SseContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String },
}

#[derive(Deserialize)]
struct SseContentBlockDelta {
    index: usize,
    delta: SseDelta,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum SseDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[derive(Deserialize)]
struct SseMessageDelta {
    delta: SseMessageDeltaInner,
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct SseMessageDeltaInner {
    stop_reason: Option<String>,
}

/// Tracks accumulated state for a single content block during streaming.
#[derive(Debug)]
enum BlockAccum {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input_json: String,
    },
}

#[async_trait]
impl Provider for AnthropicProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let model = if request.model.is_empty() {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        let (system, messages) = convert_messages(&request.messages);
        let tools = convert_tools(&request.tools);

        let api_request = AnthropicRequest {
            model,
            messages,
            max_tokens: request.max_tokens,
            system,
            temperature: if request.temperature > 0.0 {
                Some(request.temperature)
            } else {
                None
            },
            tools,
            stream: false,
        };

        debug!("sending request to Anthropic API");

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&api_request)
            .send()
            .await
            .context("failed to send request to Anthropic")?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<AnthropicError>(&body) {
                anyhow::bail!("Anthropic API error ({}): {}", status, err.error.message);
            }
            anyhow::bail!("Anthropic API error ({}): {}", status, body);
        }

        let api_response: AnthropicResponse =
            serde_json::from_str(&body).context("failed to parse Anthropic response")?;

        let mut content_text = None;
        let mut tool_calls = Vec::new();

        for block in &api_response.content {
            match block {
                AnthropicContent::Text { text } => {
                    content_text = Some(text.clone());
                }
                AnthropicContent::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: input.clone(),
                    });
                }
            }
        }

        let stop_reason = match api_response.stop_reason.as_deref() {
            Some("end_turn") => StopReason::EndTurn,
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            Some(other) => StopReason::Unknown(other.to_string()),
            None => StopReason::EndTurn,
        };

        Ok(ChatResponse {
            content: content_text,
            tool_calls,
            usage: Usage {
                prompt_tokens: api_response.usage.input_tokens,
                completion_tokens: api_response.usage.output_tokens,
            },
            stop_reason,
        })
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        let model = if request.model.is_empty() {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        let (system, messages) = convert_messages(&request.messages);
        let tools = convert_tools(&request.tools);

        let api_request = AnthropicRequest {
            model,
            messages,
            max_tokens: request.max_tokens,
            system,
            temperature: if request.temperature > 0.0 {
                Some(request.temperature)
            } else {
                None
            },
            tools,
            stream: true,
        };

        debug!("sending streaming request to Anthropic API");

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&api_request)
            .send()
            .await
            .context("failed to send streaming request to Anthropic")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            if let Ok(err) = serde_json::from_str::<AnthropicError>(&body) {
                anyhow::bail!("Anthropic API error ({}): {}", status, err.error.message);
            }
            anyhow::bail!("Anthropic API error ({}): {}", status, body);
        }

        // State accumulators for building the final ChatResponse.
        let mut blocks: Vec<BlockAccum> = Vec::new();
        let mut usage = Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
        };
        let mut stop_reason = StopReason::EndTurn;

        // Read the SSE byte stream and process line by line.
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("error reading streaming response chunk")?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete lines from the buffer.
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                buffer.drain(..=newline_pos);

                // Skip empty lines and event type lines.
                if line.is_empty() || line.starts_with("event:") {
                    continue;
                }

                let data = match line.strip_prefix("data: ") {
                    Some(d) => d,
                    None => continue,
                };

                // End of stream signal.
                if data == "[DONE]" {
                    break;
                }

                // Parse the event type first.
                let event: SseEvent = match serde_json::from_str(data) {
                    Ok(e) => e,
                    Err(e) => {
                        debug!("failed to parse SSE event type: {e}, data: {data}");
                        continue;
                    }
                };

                match event.event_type.as_str() {
                    "message_start" => {
                        if let Ok(msg) = serde_json::from_str::<SseMessageStart>(data) {
                            if let Some(u) = msg.message.usage {
                                usage.prompt_tokens = u.input_tokens;
                                usage.completion_tokens = u.output_tokens;
                                let _ = tx.send(StreamEvent::Usage(usage.clone())).await;
                            }
                        }
                    }

                    "content_block_start" => {
                        if let Ok(cbs) = serde_json::from_str::<SseContentBlockStart>(data) {
                            // Ensure blocks vec is large enough.
                            while blocks.len() <= cbs.index {
                                blocks.push(BlockAccum::Text(String::new()));
                            }
                            match cbs.content_block {
                                SseContentBlock::Text { text } => {
                                    blocks[cbs.index] = BlockAccum::Text(text);
                                }
                                SseContentBlock::ToolUse { id, name } => {
                                    let _ = tx
                                        .send(StreamEvent::ToolUseStart {
                                            id: id.clone(),
                                            name: name.clone(),
                                        })
                                        .await;
                                    blocks[cbs.index] = BlockAccum::ToolUse {
                                        id,
                                        name,
                                        input_json: String::new(),
                                    };
                                }
                            }
                        }
                    }

                    "content_block_delta" => {
                        if let Ok(cbd) = serde_json::from_str::<SseContentBlockDelta>(data) {
                            match cbd.delta {
                                SseDelta::TextDelta { text } => {
                                    let _ = tx.send(StreamEvent::TextDelta(text.clone())).await;
                                    if let Some(BlockAccum::Text(accum)) =
                                        blocks.get_mut(cbd.index)
                                    {
                                        accum.push_str(&text);
                                    }
                                }
                                SseDelta::InputJsonDelta { partial_json } => {
                                    let _ = tx
                                        .send(StreamEvent::ToolUseInput(partial_json.clone()))
                                        .await;
                                    if let Some(BlockAccum::ToolUse {
                                        input_json, ..
                                    }) = blocks.get_mut(cbd.index)
                                    {
                                        input_json.push_str(&partial_json);
                                    }
                                }
                            }
                        }
                    }

                    "message_delta" => {
                        if let Ok(md) = serde_json::from_str::<SseMessageDelta>(data) {
                            stop_reason = match md.delta.stop_reason.as_deref() {
                                Some("end_turn") => StopReason::EndTurn,
                                Some("tool_use") => StopReason::ToolUse,
                                Some("max_tokens") => StopReason::MaxTokens,
                                Some(other) => StopReason::Unknown(other.to_string()),
                                None => StopReason::EndTurn,
                            };
                            if let Some(u) = md.usage {
                                usage.completion_tokens = u.output_tokens;
                                let _ = tx.send(StreamEvent::Usage(usage.clone())).await;
                            }
                        }
                    }

                    "message_stop" => {
                        // Will assemble and emit MessageComplete below.
                    }

                    "content_block_stop" | "ping" => {
                        // No action needed.
                    }

                    other => {
                        debug!("unknown SSE event type: {other}");
                    }
                }
            }
        }

        // Assemble final ChatResponse from accumulated blocks.
        let mut content_text: Option<String> = None;
        let mut tool_calls = Vec::new();

        for block in blocks {
            match block {
                BlockAccum::Text(text) => {
                    if !text.is_empty() {
                        content_text = Some(text);
                    }
                }
                BlockAccum::ToolUse {
                    id,
                    name,
                    input_json,
                } => {
                    let arguments: serde_json::Value = serde_json::from_str(&input_json)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments,
                    });
                }
            }
        }

        let response = ChatResponse {
            content: content_text,
            tool_calls,
            usage: usage.clone(),
            stop_reason,
        };

        let _ = tx.send(StreamEvent::MessageComplete(response)).await;

        Ok(())
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    async fn health_check(&self) -> Result<()> {
        // Try a minimal request.
        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "model": &self.default_model,
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 1,
            }))
            .send()
            .await?;

        if response.status().is_success() || response.status().as_u16() == 400 {
            Ok(()) // 400 = bad request but API key is valid.
        } else {
            anyhow::bail!("Anthropic health check failed: {}", response.status());
        }
    }
}
