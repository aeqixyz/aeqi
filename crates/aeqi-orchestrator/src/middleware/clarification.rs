//! Clarification Middleware — enables clean agent-to-human interruption.
//!
//! When a worker needs human input (missing information, a choice between
//! options, or a simple confirmation), it can invoke the `ask_clarification`
//! tool. This middleware intercepts that tool call, parses the structured
//! request, stores it in [`WorkerContext::metadata`], and halts execution
//! cleanly so the worker pool can surface the question to the user.
//!
//! Implements Priority 7 from the AEQI v4 synthesis: "Clarification Interruption."
//! Inspired by Deer Flow's structured agent-to-human handoff pattern.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::{Middleware, MiddlewareAction, ORDER_CLARIFICATION, ToolCall, WorkerContext};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The type of clarification being requested.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ClarificationType {
    /// Worker needs information that was not provided.
    MissingInfo,
    /// Worker needs the user to choose between options.
    Choice,
    /// Worker needs a yes/no confirmation before proceeding.
    Confirmation,
}

/// A structured clarification request from a worker to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationRequest {
    /// What kind of clarification is needed.
    pub clarification_type: ClarificationType,
    /// The question to present to the user.
    pub question: String,
    /// Optional additional context to help the user answer.
    pub context: Option<String>,
    /// Available options (for Choice type; may be empty for others).
    pub options: Vec<String>,
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Clarification middleware — intercepts clarification tool calls and halts
/// execution with a structured question for the user.
pub struct ClarificationMiddleware {
    /// Name of the tool that triggers clarification. Default: "ask_clarification".
    trigger_tool_name: String,
}

impl ClarificationMiddleware {
    /// Create with default trigger tool name ("ask_clarification").
    pub fn new() -> Self {
        Self {
            trigger_tool_name: "ask_clarification".into(),
        }
    }

    /// Create with a custom trigger tool name.
    pub fn with_trigger(tool_name: impl Into<String>) -> Self {
        Self {
            trigger_tool_name: tool_name.into(),
        }
    }

    /// Try to parse a `ClarificationRequest` from the tool call input.
    ///
    /// Falls back to treating the entire input as a plain-text question if
    /// JSON parsing fails.
    fn parse_request(input: &str) -> ClarificationRequest {
        // Try structured JSON first.
        if let Ok(req) = serde_json::from_str::<ClarificationRequest>(input) {
            return req;
        }

        // Fallback: treat the input as a plain-text question.
        ClarificationRequest {
            clarification_type: ClarificationType::MissingInfo,
            question: input.to_string(),
            context: None,
            options: Vec::new(),
        }
    }
}

impl Default for ClarificationMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for ClarificationMiddleware {
    fn name(&self) -> &str {
        "clarification"
    }

    fn order(&self) -> u32 {
        ORDER_CLARIFICATION
    }

    async fn before_tool(&self, ctx: &mut WorkerContext, call: &ToolCall) -> MiddlewareAction {
        if call.name != self.trigger_tool_name {
            return MiddlewareAction::Continue;
        }

        let request = Self::parse_request(&call.input);
        let question = request.question.clone();

        // Serialize the structured request into metadata for the worker_pool.
        match serde_json::to_string(&request) {
            Ok(json) => {
                ctx.metadata.insert("clarification_request".into(), json);
            }
            Err(e) => {
                warn!(
                    task_id = %ctx.task_id,
                    error = %e,
                    "failed to serialize clarification request — storing question only"
                );
                ctx.metadata
                    .insert("clarification_request".into(), question.clone());
            }
        }

        info!(
            task_id = %ctx.task_id,
            clarification_type = ?request.clarification_type,
            question = %question,
            options = ?request.options,
            "clarification requested — halting execution"
        );

        MiddlewareAction::Halt(format!("Clarification needed: {question}"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> WorkerContext {
        WorkerContext::new("task-1", "test task", "engineer", "aeqi")
    }

    fn make_call(name: &str, input: &str) -> ToolCall {
        ToolCall {
            name: name.into(),
            input: input.into(),
        }
    }

    #[tokio::test]
    async fn non_clarification_tool_passes_through() {
        let mw = ClarificationMiddleware::new();
        let mut ctx = test_ctx();
        let call = make_call("Bash", "ls -la");

        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(
            matches!(action, MiddlewareAction::Continue),
            "expected Continue for non-clarification tool, got {action:?}"
        );
        assert!(!ctx.metadata.contains_key("clarification_request"));
    }

    #[tokio::test]
    async fn clarification_tool_halts_with_message() {
        let mw = ClarificationMiddleware::new();
        let mut ctx = test_ctx();
        let input = serde_json::to_string(&ClarificationRequest {
            clarification_type: ClarificationType::MissingInfo,
            question: "What database should I use?".into(),
            context: Some("The task requires a database but none was specified.".into()),
            options: Vec::new(),
        })
        .unwrap();
        let call = make_call("ask_clarification", &input);

        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(
            matches!(
                action,
                MiddlewareAction::Halt(ref s) if s.contains("What database should I use?")
            ),
            "expected Halt with question, got {action:?}"
        );
    }

    #[tokio::test]
    async fn request_stored_in_metadata() {
        let mw = ClarificationMiddleware::new();
        let mut ctx = test_ctx();
        let request = ClarificationRequest {
            clarification_type: ClarificationType::Choice,
            question: "Which layout?".into(),
            context: None,
            options: vec!["Grid".into(), "List".into()],
        };
        let input = serde_json::to_string(&request).unwrap();
        let call = make_call("ask_clarification", &input);

        let _ = mw.before_tool(&mut ctx, &call).await;

        let stored = ctx.metadata.get("clarification_request");
        assert!(
            stored.is_some(),
            "clarification_request should be in metadata"
        );

        let parsed: ClarificationRequest = serde_json::from_str(stored.unwrap()).unwrap();
        assert_eq!(parsed.clarification_type, ClarificationType::Choice);
        assert_eq!(parsed.question, "Which layout?");
        assert_eq!(parsed.options, vec!["Grid", "List"]);
    }

    #[tokio::test]
    async fn different_clarification_types_work() {
        let mw = ClarificationMiddleware::new();

        // MissingInfo
        {
            let mut ctx = test_ctx();
            let input = serde_json::to_string(&ClarificationRequest {
                clarification_type: ClarificationType::MissingInfo,
                question: "What API key?".into(),
                context: None,
                options: Vec::new(),
            })
            .unwrap();
            let call = make_call("ask_clarification", &input);
            let action = mw.before_tool(&mut ctx, &call).await;
            assert!(matches!(action, MiddlewareAction::Halt(_)));

            let parsed: ClarificationRequest =
                serde_json::from_str(ctx.metadata.get("clarification_request").unwrap()).unwrap();
            assert_eq!(parsed.clarification_type, ClarificationType::MissingInfo);
        }

        // Choice
        {
            let mut ctx = test_ctx();
            let input = serde_json::to_string(&ClarificationRequest {
                clarification_type: ClarificationType::Choice,
                question: "Which one?".into(),
                context: None,
                options: vec!["A".into(), "B".into()],
            })
            .unwrap();
            let call = make_call("ask_clarification", &input);
            let action = mw.before_tool(&mut ctx, &call).await;
            assert!(matches!(action, MiddlewareAction::Halt(_)));

            let parsed: ClarificationRequest =
                serde_json::from_str(ctx.metadata.get("clarification_request").unwrap()).unwrap();
            assert_eq!(parsed.clarification_type, ClarificationType::Choice);
        }

        // Confirmation
        {
            let mut ctx = test_ctx();
            let input = serde_json::to_string(&ClarificationRequest {
                clarification_type: ClarificationType::Confirmation,
                question: "Deploy to production?".into(),
                context: Some("This will affect live users.".into()),
                options: Vec::new(),
            })
            .unwrap();
            let call = make_call("ask_clarification", &input);
            let action = mw.before_tool(&mut ctx, &call).await;
            assert!(matches!(action, MiddlewareAction::Halt(_)));

            let parsed: ClarificationRequest =
                serde_json::from_str(ctx.metadata.get("clarification_request").unwrap()).unwrap();
            assert_eq!(parsed.clarification_type, ClarificationType::Confirmation);
            assert_eq!(
                parsed.context.as_deref(),
                Some("This will affect live users.")
            );
        }
    }

    #[tokio::test]
    async fn empty_options_accepted() {
        let mw = ClarificationMiddleware::new();
        let mut ctx = test_ctx();
        let input = serde_json::to_string(&ClarificationRequest {
            clarification_type: ClarificationType::MissingInfo,
            question: "What environment?".into(),
            context: None,
            options: Vec::new(),
        })
        .unwrap();
        let call = make_call("ask_clarification", &input);

        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Halt(_)));

        let parsed: ClarificationRequest =
            serde_json::from_str(ctx.metadata.get("clarification_request").unwrap()).unwrap();
        assert!(parsed.options.is_empty());
    }

    #[tokio::test]
    async fn plain_text_input_fallback() {
        let mw = ClarificationMiddleware::new();
        let mut ctx = test_ctx();
        // Non-JSON input should be treated as a plain-text question.
        let call = make_call("ask_clarification", "What should I do next?");

        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(
            matches!(
                action,
                MiddlewareAction::Halt(ref s) if s.contains("What should I do next?")
            ),
            "expected Halt with plain-text question, got {action:?}"
        );

        let parsed: ClarificationRequest =
            serde_json::from_str(ctx.metadata.get("clarification_request").unwrap()).unwrap();
        assert_eq!(parsed.clarification_type, ClarificationType::MissingInfo);
        assert_eq!(parsed.question, "What should I do next?");
    }

    #[tokio::test]
    async fn custom_trigger_name() {
        let mw = ClarificationMiddleware::with_trigger("request_input");
        let mut ctx = test_ctx();

        // Default name should NOT trigger.
        let call = make_call("ask_clarification", "test");
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Continue));

        // Custom name SHOULD trigger.
        let call = make_call("request_input", "What do you want?");
        let action = mw.before_tool(&mut ctx, &call).await;
        assert!(matches!(action, MiddlewareAction::Halt(_)));
    }

    #[test]
    fn default_impl() {
        let mw = ClarificationMiddleware::default();
        assert_eq!(mw.trigger_tool_name, "ask_clarification");
    }

    #[test]
    fn clarification_type_serde_roundtrip() {
        let types = vec![
            ClarificationType::MissingInfo,
            ClarificationType::Choice,
            ClarificationType::Confirmation,
        ];
        for ct in types {
            let json = serde_json::to_string(&ct).unwrap();
            let parsed: ClarificationType = serde_json::from_str(&json).unwrap();
            assert_eq!(ct, parsed);
        }
    }
}
