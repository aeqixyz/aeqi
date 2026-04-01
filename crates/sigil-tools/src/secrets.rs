//! Secrets management tool — encrypted credential store with write-only agent access.
//!
//! Security model:
//! - Agent can LIST secret names (inventory awareness)
//! - Agent can STORE secrets (set up integrations)
//! - Agent can CHECK if a secret exists (health checks)
//! - Agent can DELETE secrets (cleanup)
//! - Agent CANNOT READ secret values (prevents exfiltration via prompt injection)
//!
//! The runtime reads secrets internally for provider init, gateway tokens, etc.
//! The agent just manages the inventory without seeing raw values.

use anyhow::Result;
use async_trait::async_trait;
use sigil_core::traits::{Tool, ToolResult, ToolSpec};
use sigil_core::SecretStore;
use std::path::PathBuf;
use tracing::debug;

/// Tool for managing encrypted secrets.
pub struct SecretsTool {
    store_path: PathBuf,
}

impl SecretsTool {
    pub fn new(store_path: PathBuf) -> Self {
        Self { store_path }
    }
}

#[async_trait]
impl Tool for SecretsTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        let store = match SecretStore::open(&self.store_path) {
            Ok(s) => s,
            Err(e) => return Ok(ToolResult::error(format!("Failed to open secret store: {e}"))),
        };

        match action {
            "list" => {
                let names = store.list()?;
                if names.is_empty() {
                    Ok(ToolResult::success("No secrets stored."))
                } else {
                    let list = names
                        .iter()
                        .map(|n| format!("  - {n}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(ToolResult::success(format!(
                        "{} secrets stored:\n{list}",
                        names.len()
                    )))
                }
            }

            "store" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
                let value = args
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'value'"))?;

                // Validate name: alphanumeric + underscores only.
                if !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    return Ok(ToolResult::error(
                        "Secret name must be alphanumeric with underscores only (e.g., OPENROUTER_API_KEY).",
                    ));
                }

                store.set(name, value)?;
                debug!(name, "secret stored");
                Ok(ToolResult::success(format!(
                    "Secret '{name}' stored securely (encrypted with ChaCha20-Poly1305)."
                )))
            }

            "check" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;

                let exists = store.get(name).is_ok();
                if exists {
                    // Check file metadata for last modified time.
                    let path = self.store_path.join(format!("{name}.enc"));
                    let modified = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .map(|elapsed| {
                            let secs = elapsed.as_secs();
                            if secs < 3600 {
                                format!("{}m ago", secs / 60)
                            } else if secs < 86400 {
                                format!("{}h ago", secs / 3600)
                            } else {
                                format!("{}d ago", secs / 86400)
                            }
                        })
                        .unwrap_or_else(|| "unknown".to_string());

                    Ok(ToolResult::success(format!(
                        "Secret '{name}' exists (last updated: {modified})."
                    )))
                } else {
                    Ok(ToolResult::success(format!(
                        "Secret '{name}' does not exist."
                    )))
                }
            }

            "delete" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;

                match store.delete(name) {
                    Ok(()) => {
                        debug!(name, "secret deleted");
                        Ok(ToolResult::success(format!("Secret '{name}' deleted.")))
                    }
                    Err(e) => Ok(ToolResult::error(format!(
                        "Failed to delete '{name}': {e}"
                    ))),
                }
            }

            _ => Ok(ToolResult::error(format!(
                "Unknown action '{action}'. Use: list, store, check, delete."
            ))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "manage_secrets".to_string(),
            description: "Manage encrypted API keys and credentials. You can list, store, check, \
                and delete secrets. Secrets are encrypted with ChaCha20-Poly1305. \
                You CANNOT read secret values — only the runtime uses them internally for \
                provider authentication. Use this to set up integrations, check which \
                credentials are configured, and manage the credential inventory."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "store", "check", "delete"],
                        "description": "Action to perform"
                    },
                    "name": {
                        "type": "string",
                        "description": "Secret name (e.g., OPENROUTER_API_KEY). Required for store/check/delete."
                    },
                    "value": {
                        "type": "string",
                        "description": "Secret value to store. Required for store action only."
                    }
                },
                "required": ["action"]
            }),
        }
    }

    fn name(&self) -> &str {
        "manage_secrets"
    }

    fn is_destructive(&self, input: &serde_json::Value) -> bool {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        action == "delete"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tool() -> (SecretsTool, TempDir) {
        let dir = TempDir::new().unwrap();
        let tool = SecretsTool::new(dir.path().to_path_buf());
        (tool, dir)
    }

    #[tokio::test]
    async fn test_list_empty() {
        let (tool, _dir) = make_tool();
        let result = tool
            .execute(serde_json::json!({"action": "list"}))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("No secrets"));
    }

    #[tokio::test]
    async fn test_store_and_check() {
        let (tool, _dir) = make_tool();

        // Store a secret.
        let result = tool
            .execute(serde_json::json!({
                "action": "store",
                "name": "TEST_KEY",
                "value": "sk-test-12345"
            }))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("stored securely"));

        // Check it exists.
        let result = tool
            .execute(serde_json::json!({"action": "check", "name": "TEST_KEY"}))
            .await
            .unwrap();
        assert!(result.output.contains("exists"));

        // List shows it.
        let result = tool
            .execute(serde_json::json!({"action": "list"}))
            .await
            .unwrap();
        assert!(result.output.contains("TEST_KEY"));
    }

    #[tokio::test]
    async fn test_delete() {
        let (tool, _dir) = make_tool();

        tool.execute(serde_json::json!({
            "action": "store",
            "name": "TO_DELETE",
            "value": "value"
        }))
        .await
        .unwrap();

        let result = tool
            .execute(serde_json::json!({"action": "delete", "name": "TO_DELETE"}))
            .await
            .unwrap();
        assert!(result.output.contains("deleted"));

        let result = tool
            .execute(serde_json::json!({"action": "check", "name": "TO_DELETE"}))
            .await
            .unwrap();
        assert!(result.output.contains("does not exist"));
    }

    #[tokio::test]
    async fn test_invalid_name() {
        let (tool, _dir) = make_tool();
        let result = tool
            .execute(serde_json::json!({
                "action": "store",
                "name": "../../../etc/passwd",
                "value": "hack"
            }))
            .await
            .unwrap();
        assert!(result.is_error || result.output.contains("alphanumeric"));
    }

    #[test]
    fn test_is_destructive() {
        let tool = SecretsTool::new(PathBuf::from("/tmp"));
        assert!(tool.is_destructive(&serde_json::json!({"action": "delete"})));
        assert!(!tool.is_destructive(&serde_json::json!({"action": "list"})));
        assert!(!tool.is_destructive(&serde_json::json!({"action": "store"})));
    }
}
