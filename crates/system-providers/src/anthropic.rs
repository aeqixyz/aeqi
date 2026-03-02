use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use system_core::traits::{
    ChatRequest, ChatResponse, Message, MessageContent, ContentPart,
    Provider, Role, StopReason, ToolCall, ToolSpec, Usage,
};
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

        Self { client, api_key, default_model }
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
    ToolUse { id: String, name: String, input: serde_json::Value },
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
            Role::Assistant => {
                match &msg.content {
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
                }
            }
            Role::Tool => {
                if let MessageContent::Parts(parts) = &msg.content {
                    let mut content_blocks = Vec::new();
                    for part in parts {
                        if let ContentPart::ToolResult { tool_use_id, content, is_error } = part {
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
    tools.iter().map(|t| AnthropicTool {
        name: t.name.clone(),
        description: t.description.clone(),
        input_schema: t.input_schema.clone(),
    }).collect()
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
            temperature: if request.temperature > 0.0 { Some(request.temperature) } else { None },
            tools,
        };

        debug!("sending request to Anthropic API");

        let response = self.client
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

        let api_response: AnthropicResponse = serde_json::from_str(&body)
            .context("failed to parse Anthropic response")?;

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

    fn name(&self) -> &str { "anthropic" }

    async fn health_check(&self) -> Result<()> {
        // Try a minimal request.
        let response = self.client
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
