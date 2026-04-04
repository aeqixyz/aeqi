use async_trait::async_trait;
use std::path::PathBuf;
use tracing::debug;

use super::{Middleware, MiddlewareAction, ORDER_GRAPH_GUARDRAILS, ToolCall, WorkerContext};

/// Graph-aware guardrails: checks code graph impact before allowing edits.
/// Injects warnings when changes affect symbols with many callers.
pub struct GraphGuardrailsMiddleware {
    graph_dir: PathBuf,
    caller_threshold: usize,
}

impl GraphGuardrailsMiddleware {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self {
            graph_dir: data_dir.join("codegraph"),
            caller_threshold: 10,
        }
    }

    fn check_edit_impact(&self, project: &str, file_path: &str, _input: &str) -> Option<String> {
        let db_path = self.graph_dir.join(format!("{project}.db"));
        if !db_path.exists() {
            return None;
        }

        let store = aeqi_graph::GraphStore::open(&db_path).ok()?;

        // Try to extract what's being changed from the edit input
        // The input contains old_string and new_string — find symbols at those lines
        let nodes = store.nodes_in_file(file_path).ok()?;
        if nodes.is_empty() {
            return None;
        }

        // Check total external callers for this file
        let mut high_impact = Vec::new();
        for node in &nodes {
            if matches!(
                node.label,
                aeqi_graph::NodeLabel::File
                    | aeqi_graph::NodeLabel::Module
                    | aeqi_graph::NodeLabel::Community
                    | aeqi_graph::NodeLabel::Process
            ) {
                continue;
            }

            let incoming = store.incoming_edges(&node.id).ok()?;
            let ext_caller_count = incoming
                .iter()
                .filter(|(e, caller)| {
                    e.edge_type == aeqi_graph::EdgeType::Calls
                        && caller
                            .as_ref()
                            .map(|c| c.file_path != file_path)
                            .unwrap_or(false)
                })
                .count();

            if ext_caller_count >= self.caller_threshold {
                high_impact.push(format!(
                    "{} ({}) has {} external callers",
                    node.name, node.label, ext_caller_count
                ));
            }

            // Also check implementors for traits
            if node.label == aeqi_graph::NodeLabel::Trait {
                let impl_count = incoming
                    .iter()
                    .filter(|(e, _)| e.edge_type == aeqi_graph::EdgeType::Implements)
                    .count();
                if impl_count >= 2 {
                    high_impact.push(format!(
                        "{} (trait) has {} implementations — verify all are updated",
                        node.name, impl_count
                    ));
                }
            }
        }

        if high_impact.is_empty() {
            return None;
        }

        Some(format!(
            "Graph impact warning for {}: {}",
            file_path,
            high_impact.join("; ")
        ))
    }
}

#[async_trait]
impl Middleware for GraphGuardrailsMiddleware {
    fn name(&self) -> &str {
        "graph_guardrails"
    }

    fn order(&self) -> u32 {
        ORDER_GRAPH_GUARDRAILS
    }

    async fn before_tool(&self, ctx: &mut WorkerContext, call: &ToolCall) -> MiddlewareAction {
        // Only check edit/write tools
        if !matches!(call.name.as_str(), "edit_file" | "write_file") {
            return MiddlewareAction::Continue;
        }

        // Extract file path from tool input
        let file_path = match serde_json::from_str::<serde_json::Value>(&call.input) {
            Ok(v) => v
                .get("file_path")
                .and_then(|f| f.as_str())
                .unwrap_or("")
                .to_string(),
            Err(_) => return MiddlewareAction::Continue,
        };

        if file_path.is_empty() {
            return MiddlewareAction::Continue;
        }

        // Derive relative path from the file path
        let rel_path = file_path
            .rsplit_once(&format!("{}/", ctx.project_name))
            .map(|(_, rel)| rel.to_string())
            .unwrap_or(file_path.clone());

        if let Some(warning) = self.check_edit_impact(&ctx.project_name, &rel_path, &call.input) {
            debug!(
                project = %ctx.project_name,
                file = %rel_path,
                "graph guardrails: injecting impact warning"
            );
            // Inject warning as a system message — don't block, just inform
            return MiddlewareAction::Inject(vec![warning]);
        }

        MiddlewareAction::Continue
    }
}
