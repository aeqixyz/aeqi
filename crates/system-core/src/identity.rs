use anyhow::{Context, Result};
use std::path::Path;

/// Identity files loaded from agent + project directories.
///
/// Two-source loading:
///   - Agent personality (PERSONA, IDENTITY, PREFERENCES, MEMORY) from `agents/{name}/`
///   - Project context (AGENTS, KNOWLEDGE, HEARTBEAT) from `projects/{name}/`
///   - Shared workflow from `agents/shared/WORKFLOW.md`
#[derive(Debug, Clone, Default)]
pub struct Identity {
    /// Core personality and purpose (PERSONA.md — from agent dir).
    pub persona: Option<String>,
    /// Name, style, expertise (IDENTITY.md — from agent dir).
    pub identity: Option<String>,
    /// Operational instructions separated from personality (OPERATIONAL.md — from agent dir).
    pub operational: Option<String>,
    /// Operating instructions (AGENTS.md — from project dir).
    pub agents: Option<String>,
    /// Periodic check instructions (HEARTBEAT.md — from project dir).
    pub heartbeat: Option<String>,
    /// Persistent memories (MEMORY.md — from agent dir).
    pub memory: Option<String>,
    /// Operational knowledge and learnings (KNOWLEDGE.md — from project dir).
    pub knowledge: Option<String>,
    /// Architect's observed preferences (PREFERENCES.md — from agent dir).
    pub preferences: Option<String>,
    /// Shared workflow from agents/shared/WORKFLOW.md.
    pub shared_workflow: Option<String>,
}

impl Identity {
    /// Load identity files from an agent directory + optional project directory.
    ///
    /// Agent personality files (PERSONA, IDENTITY, PREFERENCES, MEMORY) from `agent_dir`.
    /// Project context files (AGENTS, KNOWLEDGE, HEARTBEAT) from `project_dir`.
    /// Shared workflow from `agent_dir/../shared/WORKFLOW.md`.
    pub fn load(agent_dir: &Path, domain_dir: Option<&Path>) -> Result<Self> {
        let shared_dir = agent_dir.parent().map(|p| p.join("shared"));

        Ok(Self {
            // Agent personality (PERSONA.md, falls back to SOUL.md for compat)
            persona: load_optional(agent_dir, "PERSONA.md")?
                .or(load_optional(agent_dir, "SOUL.md")?),
            identity: load_optional(agent_dir, "IDENTITY.md")?,
            operational: load_optional(agent_dir, "OPERATIONAL.md")?,
            preferences: load_optional(agent_dir, "PREFERENCES.md")?,
            memory: load_optional(agent_dir, "MEMORY.md")?,
            // Project context
            agents: domain_dir
                .map(|d| load_optional(d, "AGENTS.md"))
                .transpose()?
                .flatten(),
            knowledge: domain_dir
                .map(|d| load_optional(d, "KNOWLEDGE.md"))
                .transpose()?
                .flatten(),
            heartbeat: domain_dir
                .map(|d| load_optional(d, "HEARTBEAT.md"))
                .transpose()?
                .flatten(),
            // Shared
            shared_workflow: shared_dir
                .as_deref()
                .map(|d| load_optional(d, "WORKFLOW.md"))
                .transpose()?
                .flatten(),
        })
    }

    /// Build identity for a specific agent working on a specific project.
    ///
    /// Combines agent personality from `agents/{agent_name}/` with project context
    /// from `projects/{project_name}/`. This is the standard way to build identity
    /// for per-project team workers.
    pub fn for_worker(agent_name: &str, project_name: &str, base_dir: &Path) -> Result<Self> {
        let agent_dir = base_dir.join("agents").join(agent_name);
        let project_dir = base_dir.join("projects").join(project_name);
        // Fall back to domains/ for backward compat.
        let project_dir = if project_dir.exists() {
            project_dir
        } else {
            let fallback = base_dir.join("domains").join(project_name);
            if fallback.exists() { fallback } else { project_dir }
        };
        Self::load(&agent_dir, Some(&project_dir))
    }

    /// Load identity from a single directory (backward compat — loads all files from one dir).
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let shared_dir = dir.parent().map(|p| p.join("shared"));

        Ok(Self {
            persona: load_optional(dir, "PERSONA.md")?
                .or(load_optional(dir, "SOUL.md")?),
            identity: load_optional(dir, "IDENTITY.md")?,
            operational: load_optional(dir, "OPERATIONAL.md")?,
            agents: load_optional(dir, "AGENTS.md")?,
            heartbeat: load_optional(dir, "HEARTBEAT.md")?,
            memory: load_optional(dir, "MEMORY.md")?,
            knowledge: load_optional(dir, "KNOWLEDGE.md")?,
            preferences: load_optional(dir, "PREFERENCES.md")?,
            shared_workflow: shared_dir
                .as_deref()
                .map(|d| load_optional(d, "WORKFLOW.md"))
                .transpose()?
                .flatten(),
        })
    }

    /// Build the system prompt from identity files.
    ///
    /// Order: shared workflow -> persona -> identity -> agents -> knowledge -> preferences -> memory.
    pub fn system_prompt(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref shared) = self.shared_workflow {
            parts.push(format!("# Shared Workflow\n\n{shared}"));
        }

        if let Some(ref persona) = self.persona {
            parts.push(format!("# Persona\n\n{persona}"));
        }

        if let Some(ref identity) = self.identity {
            parts.push(format!("# Identity\n\n{identity}"));
        }

        if let Some(ref operational) = self.operational {
            parts.push(format!("# Operational Instructions\n\n{operational}"));
        }

        if let Some(ref agents) = self.agents {
            parts.push(format!("# Operating Instructions\n\n{agents}"));
        }

        if let Some(ref knowledge) = self.knowledge {
            parts.push(format!("# Project Knowledge\n\n{knowledge}"));
        }

        if let Some(ref preferences) = self.preferences {
            parts.push(format!("# Architect Preferences\n\n{preferences}"));
        }

        if let Some(ref memory) = self.memory {
            parts.push(format!("# Persistent Memory\n\n{memory}"));
        }

        if parts.is_empty() {
            "You are a helpful AI agent.".to_string()
        } else {
            parts.join("\n\n---\n\n")
        }
    }

    /// Check if any identity files are loaded.
    pub fn is_loaded(&self) -> bool {
        self.persona.is_some()
            || self.identity.is_some()
            || self.agents.is_some()
    }
}

fn load_optional(dir: &Path, filename: &str) -> Result<Option<String>> {
    let path = dir.join(filename);
    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if content.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(content))
        }
    } else {
        Ok(None)
    }
}
