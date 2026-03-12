use anyhow::Result;
use async_trait::async_trait;
use sigil_core::traits::{ToolResult, ToolSpec};
use std::path::{Path, PathBuf};
use tracing::debug;

/// File read tool with workspace scoping.
pub struct FileReadTool {
    workspace: PathBuf,
}

impl FileReadTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn validate_path(&self, path: &str) -> Result<PathBuf> {
        let resolved = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.workspace.join(path)
        };

        let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

        let workspace_canonical = self
            .workspace
            .canonicalize()
            .unwrap_or_else(|_| self.workspace.clone());

        if !canonical.starts_with(&workspace_canonical) {
            anyhow::bail!(
                "path {} is outside workspace {}",
                path,
                self.workspace.display()
            );
        }

        Ok(canonical)
    }
}

#[async_trait]
impl sigil_core::traits::Tool for FileReadTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;

        let resolved = match self.validate_path(path) {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::error(e.to_string())),
        };

        debug!(path = %resolved.display(), "reading file");

        match tokio::fs::read_to_string(&resolved).await {
            Ok(content) => {
                let mut output = content;
                if output.len() > 100_000 {
                    output.truncate(100_000);
                    output.push_str("\n... (file truncated)");
                }
                Ok(ToolResult::success(output))
            }
            Err(e) => Ok(ToolResult::error(format!(
                "failed to read {}: {e}",
                resolved.display()
            ))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_file".to_string(),
            description: "Read the contents of a file.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file (relative to workspace or absolute)"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    fn name(&self) -> &str {
        "read_file"
    }
}

/// File write tool with workspace scoping.
pub struct FileWriteTool {
    workspace: PathBuf,
}

impl FileWriteTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn validate_path(&self, path: &str) -> Result<PathBuf> {
        let resolved = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.workspace.join(path)
        };

        // For writes, the file may not exist yet; check parent directory.
        let parent = resolved.parent().unwrap_or(&resolved);

        let parent_canonical = parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf());

        let workspace_canonical = self
            .workspace
            .canonicalize()
            .unwrap_or_else(|_| self.workspace.clone());

        if !parent_canonical.starts_with(&workspace_canonical) {
            anyhow::bail!(
                "path {} is outside workspace {}",
                path,
                self.workspace.display()
            );
        }

        Ok(resolved)
    }
}

#[async_trait]
impl sigil_core::traits::Tool for FileWriteTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;

        let resolved = match self.validate_path(path) {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::error(e.to_string())),
        };

        debug!(path = %resolved.display(), bytes = content.len(), "writing file");

        // Create parent directories if needed.
        if let Some(parent) = resolved.parent()
            && let Err(e) = tokio::fs::create_dir_all(parent).await
        {
            return Ok(ToolResult::error(format!(
                "failed to create directories: {e}"
            )));
        }

        match tokio::fs::write(&resolved, content).await {
            Ok(()) => Ok(ToolResult::success(format!(
                "wrote {} bytes to {}",
                content.len(),
                resolved.display()
            ))),
            Err(e) => Ok(ToolResult::error(format!(
                "failed to write {}: {e}",
                resolved.display()
            ))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write_file".to_string(),
            description: "Write content to a file. Creates parent directories if needed."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file (relative to workspace or absolute)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    fn name(&self) -> &str {
        "write_file"
    }
}

/// Directory listing tool.
pub struct ListDirTool {
    workspace: PathBuf,
}

impl ListDirTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl sigil_core::traits::Tool for ListDirTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let resolved = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.workspace.join(path)
        };

        debug!(path = %resolved.display(), "listing directory");

        let mut entries = Vec::new();
        let mut dir = match tokio::fs::read_dir(&resolved).await {
            Ok(d) => d,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "failed to read directory {}: {e}",
                    resolved.display()
                )));
            }
        };

        while let Ok(Some(entry)) = dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().await.ok();
            let suffix = if file_type.as_ref().is_some_and(|ft| ft.is_dir()) {
                "/"
            } else {
                ""
            };
            entries.push(format!("{name}{suffix}"));
        }

        entries.sort();
        Ok(ToolResult::success(entries.join("\n")))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "list_dir".to_string(),
            description: "List files and directories in a path.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path (defaults to workspace root)"
                    }
                }
            }),
        }
    }

    fn name(&self) -> &str {
        "list_dir"
    }
}
