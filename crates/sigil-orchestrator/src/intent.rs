//! LLM-backed intent classification for chat messages.
//!
//! Uses a cheap model (e.g. Gemini Flash via OpenRouter) to classify ambiguous
//! messages that don't match keyword fast paths. Falls back gracefully to
//! "unknown" if the API call fails.

use serde::Deserialize;
use tracing::{debug, warn};

/// Classified intent from a chat message.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatIntent {
    /// "create task to...", "build a...", "implement..."
    CreateTask,
    /// "close task X", "mark X done"
    CloseTask,
    /// "note: ...", "remember: ..."
    BlackboardPost,
    /// "status", "what's going on", "how are things"
    StatusQuery,
    /// Complex request that should go to the full path (agent execution).
    FullPath,
    /// Ambiguous or unrecognizable — default to full path.
    Unknown,
}

#[derive(Debug, Deserialize)]
struct ClassifierOutput {
    intent: String,
}

/// Classifies chat messages into intents using keyword matching (fast path)
/// and optional LLM classification (slow path) for ambiguous messages.
pub struct IntentClassifier {
    api_key: String,
    client: reqwest::Client,
    model: String,
}

impl IntentClassifier {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .expect("failed to build reqwest client"),
            model,
        }
    }

    /// Classify a message. Tries keyword fast path first, falls back to LLM.
    pub async fn classify(&self, message: &str) -> ChatIntent {
        // Fast path: exact keyword matching (no API call).
        if let Some(intent) = self.keyword_match(message) {
            return intent;
        }

        // Slow path: LLM classification for ambiguous messages.
        if self.api_key.is_empty() {
            return ChatIntent::Unknown;
        }

        self.llm_classify(message).await
    }

    /// Fast path: keyword-based intent detection. Returns None if ambiguous.
    fn keyword_match(&self, message: &str) -> Option<ChatIntent> {
        let lower = message.to_lowercase();

        // Create task — explicit prefixes.
        if lower.starts_with("create task")
            || lower.starts_with("new task")
            || lower.starts_with("add task")
        {
            return Some(ChatIntent::CreateTask);
        }

        // Close task — explicit prefixes.
        if lower.starts_with("close task") || lower.starts_with("done with") {
            return Some(ChatIntent::CloseTask);
        }

        // Blackboard / notes — explicit prefixes.
        if lower.starts_with("note:")
            || lower.starts_with("remember:")
            || lower.starts_with("blackboard:")
        {
            return Some(ChatIntent::BlackboardPost);
        }

        // Status queries — common patterns.
        if lower == "status"
            || lower == "what's the status"
            || lower == "what's going on"
            || lower.starts_with("status of")
        {
            return Some(ChatIntent::StatusQuery);
        }

        // No keyword match — ambiguous.
        None
    }

    /// Slow path: LLM-backed classification.
    async fn llm_classify(&self, message: &str) -> ChatIntent {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": "You classify user messages for an AI orchestrator. Respond with ONLY a JSON object (no markdown).\n\nIntents:\n- \"create_task\": User wants to create, build, implement, fix, add, or do something that requires work\n- \"close_task\": User wants to close, finish, mark done a specific task\n- \"note\": User wants to save a note, remember something, post to blackboard\n- \"status\": User asks about status, progress, what's happening\n- \"full\": Complex request, multi-step, or unclear — needs full agent execution\n\nRespond: {\"intent\": \"<intent>\"}"
                },
                {"role": "user", "content": message}
            ],
            "max_tokens": 30,
            "temperature": 0.0
        });

        let response = self
            .client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await;

        match response {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(v) => {
                    let text = v
                        .pointer("/choices/0/message/content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("");

                    // Try to parse JSON response.
                    let intent_str =
                        if let Ok(parsed) = serde_json::from_str::<ClassifierOutput>(text) {
                            parsed.intent
                        } else {
                            // Fallback: try to extract intent from raw text.
                            text.to_lowercase()
                        };

                    let result = match intent_str.as_str() {
                        "create_task" => ChatIntent::CreateTask,
                        "close_task" => ChatIntent::CloseTask,
                        "note" => ChatIntent::BlackboardPost,
                        "status" => ChatIntent::StatusQuery,
                        "full" => ChatIntent::FullPath,
                        _ => ChatIntent::Unknown,
                    };

                    debug!(
                        message = %message.chars().take(60).collect::<String>(),
                        intent = ?result,
                        "LLM intent classified"
                    );
                    result
                }
                Err(e) => {
                    warn!("intent classifier parse error: {e}");
                    ChatIntent::Unknown
                }
            },
            Err(e) => {
                warn!("intent classifier API error: {e}");
                ChatIntent::Unknown
            }
        }
    }
}
