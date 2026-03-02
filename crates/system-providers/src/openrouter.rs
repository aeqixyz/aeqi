use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use system_core::traits::{
    ChatRequest, ChatResponse, Provider, StopReason, ToolCall, ToolSpec, Usage,
};
use tracing::debug;

const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// OpenRouter LLM provider.
pub struct OpenRouterProvider {
    client: Client,
    api_key: String,
    default_model: String,
}

impl OpenRouterProvider {
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

    pub fn default_model(&self) -> &str {
        &self.default_model
    }
}

// --- OpenRouter API types ---

#[derive(Debug, Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ApiTool>,
    max_tokens: u32,
    temperature: f32,
    /// OpenRouter provider routing options.
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<ProviderRouting>,
}

#[derive(Debug, Serialize)]
struct ProviderRouting {
    /// Disable content moderation on the provider side.
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_fallbacks: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ApiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiTool {
    r#type: String,
    function: ApiFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ApiToolCall {
    id: String,
    r#type: String,
    function: ApiToolCallFunction,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ApiToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    choices: Vec<ApiChoice>,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct ApiChoice {
    message: ApiChoiceMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiChoiceMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    error: ApiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    message: String,
    #[serde(default)]
    code: Option<String>,
}

// --- Conversion helpers ---

fn convert_messages(messages: &[system_core::traits::Message]) -> Vec<ApiMessage> {
    use system_core::traits::{ContentPart, MessageContent, Role};

    let mut api_messages = Vec::new();

    for msg in messages {
        match &msg.role {
            Role::System | Role::User => {
                let content = match &msg.content {
                    MessageContent::Text(t) => Some(serde_json::Value::String(t.clone())),
                    MessageContent::Parts(parts) => {
                        let text: String = parts
                            .iter()
                            .filter_map(|p| match p {
                                ContentPart::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("");
                        Some(serde_json::Value::String(text))
                    }
                };
                api_messages.push(ApiMessage {
                    role: match msg.role {
                        Role::System => "system".to_string(),
                        _ => "user".to_string(),
                    },
                    content,
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
            Role::Assistant => {
                match &msg.content {
                    MessageContent::Text(t) => {
                        api_messages.push(ApiMessage {
                            role: "assistant".to_string(),
                            content: Some(serde_json::Value::String(t.clone())),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                    MessageContent::Parts(parts) => {
                        let text: Option<String> = {
                            let texts: Vec<&str> = parts
                                .iter()
                                .filter_map(|p| match p {
                                    ContentPart::Text { text } => Some(text.as_str()),
                                    _ => None,
                                })
                                .collect();
                            if texts.is_empty() {
                                None
                            } else {
                                Some(texts.join(""))
                            }
                        };

                        let tool_calls: Vec<ApiToolCall> = parts
                            .iter()
                            .filter_map(|p| match p {
                                ContentPart::ToolUse { id, name, input } => Some(ApiToolCall {
                                    id: id.clone(),
                                    r#type: "function".to_string(),
                                    function: ApiToolCallFunction {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input)
                                            .unwrap_or_default(),
                                    },
                                }),
                                _ => None,
                            })
                            .collect();

                        api_messages.push(ApiMessage {
                            role: "assistant".to_string(),
                            content: text.map(serde_json::Value::String),
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls)
                            },
                            tool_call_id: None,
                        });
                    }
                }
            }
            Role::Tool => {
                // Tool results: each ToolResult part becomes a separate message.
                if let MessageContent::Parts(parts) = &msg.content {
                    for part in parts {
                        if let ContentPart::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } = part
                        {
                            api_messages.push(ApiMessage {
                                role: "tool".to_string(),
                                content: Some(serde_json::Value::String(content.clone())),
                                tool_calls: None,
                                tool_call_id: Some(tool_use_id.clone()),
                            });
                        }
                    }
                }
            }
        }
    }

    api_messages
}

fn convert_tools(tools: &[ToolSpec]) -> Vec<ApiTool> {
    tools
        .iter()
        .map(|t| ApiTool {
            r#type: "function".to_string(),
            function: ApiFunction {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            },
        })
        .collect()
}

#[async_trait]
impl Provider for OpenRouterProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let model = if request.model.is_empty() {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        let api_request = ApiRequest {
            model,
            messages: convert_messages(&request.messages),
            tools: convert_tools(&request.tools),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            provider: Some(ProviderRouting {
                allow_fallbacks: Some(false),
            }),
        };

        debug!(
            provider = "openrouter",
            model = %api_request.model,
            messages = api_request.messages.len(),
            tools = api_request.tools.len(),
            "sending request"
        );

        let response = self
            .client
            .post(OPENROUTER_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://sigil.dev")
            .header("X-Title", "Realm Agent")
            .json(&api_request)
            .send()
            .await
            .context("failed to send request to OpenRouter")?;

        let status = response.status();
        let body = response.text().await.context("failed to read response body")?;

        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<ApiError>(&body) {
                anyhow::bail!(
                    "OpenRouter API error ({}): {}",
                    err.error.code.unwrap_or_default(),
                    err.error.message
                );
            }
            anyhow::bail!("OpenRouter API error ({}): {}", status, body);
        }

        let api_response: ApiResponse =
            serde_json::from_str(&body).context("failed to parse OpenRouter response")?;

        let choice = api_response
            .choices
            .into_iter()
            .next()
            .context("no choices in OpenRouter response")?;

        let tool_calls: Vec<ToolCall> = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                let arguments: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);
                ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments,
                }
            })
            .collect();

        let stop_reason = match choice.finish_reason.as_deref() {
            Some("stop") => StopReason::EndTurn,
            Some("tool_calls") => StopReason::ToolUse,
            Some("length") => StopReason::MaxTokens,
            Some(other) => StopReason::Unknown(other.to_string()),
            None => {
                if tool_calls.is_empty() {
                    StopReason::EndTurn
                } else {
                    StopReason::ToolUse
                }
            }
        };

        let usage = api_response.usage.map(|u| Usage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
        }).unwrap_or_default();

        Ok(ChatResponse {
            content: choice.message.content,
            tool_calls,
            usage,
            stop_reason,
        })
    }

    fn name(&self) -> &str {
        "openrouter"
    }

    async fn health_check(&self) -> Result<()> {
        let response = self
            .client
            .get("https://openrouter.ai/api/v1/models")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("failed to reach OpenRouter")?;

        if response.status().is_success() {
            Ok(())
        } else {
            anyhow::bail!("OpenRouter health check failed: {}", response.status())
        }
    }
}
