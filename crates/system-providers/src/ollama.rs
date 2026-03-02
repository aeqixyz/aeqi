use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use system_core::traits::{
    ChatRequest, ChatResponse, Message, MessageContent, ContentPart,
    Provider, Role, StopReason, ToolCall, ToolSpec, Usage,
};
use tracing::debug;

/// Ollama local model provider (OpenAI-compatible API).
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    default_model: String,
}

impl OllamaProvider {
    pub fn new(base_url: String, default_model: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300)) // Local models can be slow.
            .build()
            .expect("failed to build HTTP client");

        Self { client, base_url, default_model }
    }

    /// Default localhost URL.
    pub fn localhost(model: String) -> Self {
        Self::new("http://localhost:11434".to_string(), model)
    }
}

// --- Ollama API types (OpenAI-compatible) ---

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OllamaTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Serialize, Deserialize)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Serialize, Deserialize)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OllamaFunction,
}

#[derive(Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
    #[serde(default)]
    prompt_eval_count: u32,
    #[serde(default)]
    eval_count: u32,
    done: bool,
}

fn convert_messages(messages: &[Message]) -> Vec<OllamaMessage> {
    messages.iter().filter_map(|msg| {
        match msg.role {
            Role::System => {
                msg.content.as_text().map(|text| OllamaMessage {
                    role: "system".to_string(),
                    content: text.to_string(),
                    tool_calls: None,
                })
            }
            Role::User => {
                msg.content.as_text().map(|text| OllamaMessage {
                    role: "user".to_string(),
                    content: text.to_string(),
                    tool_calls: None,
                })
            }
            Role::Assistant => {
                match &msg.content {
                    MessageContent::Text(text) => Some(OllamaMessage {
                        role: "assistant".to_string(),
                        content: text.clone(),
                        tool_calls: None,
                    }),
                    MessageContent::Parts(parts) => {
                        let mut text = String::new();
                        let mut tool_calls = Vec::new();
                        for part in parts {
                            match part {
                                ContentPart::Text { text: t } => text.push_str(t),
                                ContentPart::ToolUse { name, input, .. } => {
                                    tool_calls.push(OllamaToolCall {
                                        function: OllamaFunctionCall {
                                            name: name.clone(),
                                            arguments: input.clone(),
                                        },
                                    });
                                }
                                _ => {}
                            }
                        }
                        Some(OllamaMessage {
                            role: "assistant".to_string(),
                            content: text,
                            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                        })
                    }
                }
            }
            Role::Tool => {
                // Ollama expects tool responses as user messages with the result.
                if let MessageContent::Parts(parts) = &msg.content {
                    let content: String = parts.iter().filter_map(|p| {
                        if let ContentPart::ToolResult { content, .. } = p {
                            Some(content.as_str())
                        } else {
                            None
                        }
                    }).collect::<Vec<_>>().join("\n");
                    Some(OllamaMessage {
                        role: "tool".to_string(),
                        content,
                        tool_calls: None,
                    })
                } else {
                    None
                }
            }
        }
    }).collect()
}

fn convert_tools(tools: &[ToolSpec]) -> Vec<OllamaTool> {
    tools.iter().map(|t| OllamaTool {
        tool_type: "function".to_string(),
        function: OllamaFunction {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.input_schema.clone(),
        },
    }).collect()
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let model = if request.model.is_empty() {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        let messages = convert_messages(&request.messages);
        let tools = convert_tools(&request.tools);

        let api_request = OllamaRequest {
            model,
            messages,
            stream: false,
            tools,
            options: Some(OllamaOptions {
                temperature: if request.temperature > 0.0 { Some(request.temperature) } else { None },
                num_predict: Some(request.max_tokens),
            }),
        };

        let url = format!("{}/api/chat", self.base_url);
        debug!(url = %url, "sending request to Ollama");

        let response = self.client
            .post(&url)
            .json(&api_request)
            .send()
            .await
            .context("failed to send request to Ollama")?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("Ollama API error ({}): {}", status, body);
        }

        let api_response: OllamaResponse = serde_json::from_str(&body)
            .context("failed to parse Ollama response")?;

        let content = if api_response.message.content.is_empty() {
            None
        } else {
            Some(api_response.message.content.clone())
        };

        let tool_calls = api_response.message.tool_calls
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, tc)| ToolCall {
                id: format!("call_{i}"),
                name: tc.function.name,
                arguments: tc.function.arguments,
            })
            .collect();

        let stop_reason = if api_response.done {
            StopReason::EndTurn
        } else {
            StopReason::ToolUse
        };

        Ok(ChatResponse {
            content,
            tool_calls,
            usage: Usage {
                prompt_tokens: api_response.prompt_eval_count,
                completion_tokens: api_response.eval_count,
            },
            stop_reason,
        })
    }

    fn name(&self) -> &str { "ollama" }

    async fn health_check(&self) -> Result<()> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self.client.get(&url).send().await?;
        if response.status().is_success() {
            Ok(())
        } else {
            anyhow::bail!("Ollama health check failed: {}", response.status());
        }
    }
}
