use anyhow::Result;
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
    /// Accumulated evolution patches (EVOLUTION.md — from agent dir, written by lifecycle engine).
    pub evolution: Option<String>,
    /// Persistent memories (MEMORY.md — from agent dir).
    pub memory: Option<String>,
    /// Operational knowledge and learnings (KNOWLEDGE.md — from project dir).
    pub knowledge: Option<String>,
    /// Architect's observed preferences (PREFERENCES.md — from agent dir).
    pub preferences: Option<String>,
    /// Shared workflow from agents/shared/WORKFLOW.md.
    pub shared_workflow: Option<String>,
    /// Skill-specific system prompt (injected at runtime from skill TOML).
    pub skill_prompt: Option<String>,
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
            persona: load_optional(agent_dir, "PERSONA.md")?,
            identity: load_optional(agent_dir, "IDENTITY.md")?,
            operational: load_optional(agent_dir, "OPERATIONAL.md")?,
            evolution: load_optional(agent_dir, "EVOLUTION.md")?,
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
            // Injected at runtime by supervisor when task has a skill.
            skill_prompt: None,
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
        Self::load(&agent_dir, Some(&project_dir))
    }

    /// Load identity from a single directory (loads all files from one dir).
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let shared_dir = dir.parent().map(|p| p.join("shared"));

        Ok(Self {
            persona: load_optional(dir, "PERSONA.md")?,
            identity: load_optional(dir, "IDENTITY.md")?,
            operational: load_optional(dir, "OPERATIONAL.md")?,
            evolution: load_optional(dir, "EVOLUTION.md")?,
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
            skill_prompt: None,
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

        if let Some(ref evolution) = self.evolution {
            parts.push(format!("# Evolution\n\n{evolution}"));
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

        if let Some(ref skill) = self.skill_prompt {
            parts.push(format!("# Active Skill\n\n{skill}"));
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
        self.persona.is_some() || self.identity.is_some() || self.agents.is_some()
    }
}

fn load_optional(dir: &Path, filename: &str) -> Result<Option<String>> {
    let path = dir.join(filename);
    match std::fs::read_to_string(&path) {
        Ok(content) if content.trim().is_empty() => Ok(None),
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::anyhow!("failed to read {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_file(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn test_load_two_source() {
        let base = tempdir().unwrap();
        let agents = base.path().join("agents");
        let agent_dir = agents.join("alice");
        let shared_dir = agents.join("shared");
        let project_dir = base.path().join("projects/myproject");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::create_dir_all(&shared_dir).unwrap();
        std::fs::create_dir_all(&project_dir).unwrap();

        write_file(&agent_dir, "PERSONA.md", "I am Alice.");
        write_file(&agent_dir, "IDENTITY.md", "Role: strategist");
        write_file(&agent_dir, "MEMORY.md", "Met the user on day 1.");
        write_file(&shared_dir, "WORKFLOW.md", "Always test first.");
        write_file(&project_dir, "AGENTS.md", "Follow the plan.");
        write_file(&project_dir, "KNOWLEDGE.md", "Project uses Rust.");

        let id = Identity::load(&agent_dir, Some(&project_dir)).unwrap();

        assert_eq!(id.persona.as_deref(), Some("I am Alice."));
        assert_eq!(id.identity.as_deref(), Some("Role: strategist"));
        assert_eq!(id.memory.as_deref(), Some("Met the user on day 1."));
        assert_eq!(id.shared_workflow.as_deref(), Some("Always test first."));
        assert_eq!(id.agents.as_deref(), Some("Follow the plan."));
        assert_eq!(id.knowledge.as_deref(), Some("Project uses Rust."));
        assert!(id.operational.is_none());
        assert!(id.evolution.is_none());
        assert!(id.heartbeat.is_none());
    }

    #[test]
    fn test_empty_files_are_none() {
        let base = tempdir().unwrap();
        let agent_dir = base.path().join("agents/empty");
        std::fs::create_dir_all(&agent_dir).unwrap();

        write_file(&agent_dir, "PERSONA.md", "  \n  ");
        write_file(&agent_dir, "MEMORY.md", "");

        let id = Identity::load(&agent_dir, None).unwrap();
        assert!(id.persona.is_none(), "whitespace-only file should be None");
        assert!(id.memory.is_none(), "empty file should be None");
    }

    #[test]
    fn test_system_prompt_order() {
        let id = Identity {
            shared_workflow: Some("workflow".into()),
            persona: Some("persona".into()),
            identity: Some("identity".into()),
            evolution: Some("evolution".into()),
            operational: Some("operational".into()),
            agents: Some("agents".into()),
            knowledge: Some("knowledge".into()),
            preferences: Some("preferences".into()),
            memory: Some("memory".into()),
            heartbeat: None,
            skill_prompt: Some("skill".into()),
        };

        let prompt = id.system_prompt();

        // Verify correct section order.
        let workflow_pos = prompt.find("# Shared Workflow").unwrap();
        let persona_pos = prompt.find("# Persona").unwrap();
        let identity_pos = prompt.find("# Identity").unwrap();
        let evolution_pos = prompt.find("# Evolution").unwrap();
        let operational_pos = prompt.find("# Operational Instructions").unwrap();
        let agents_pos = prompt.find("# Operating Instructions").unwrap();
        let knowledge_pos = prompt.find("# Project Knowledge").unwrap();
        let skill_pos = prompt.find("# Active Skill").unwrap();
        let preferences_pos = prompt.find("# Architect Preferences").unwrap();
        let memory_pos = prompt.find("# Persistent Memory").unwrap();

        assert!(workflow_pos < persona_pos);
        assert!(persona_pos < identity_pos);
        assert!(identity_pos < evolution_pos);
        assert!(evolution_pos < operational_pos);
        assert!(operational_pos < agents_pos);
        assert!(agents_pos < knowledge_pos);
        assert!(knowledge_pos < skill_pos);
        assert!(skill_pos < preferences_pos);
        assert!(preferences_pos < memory_pos);

        // Sections separated by ---
        assert!(prompt.contains("\n\n---\n\n"));
    }

    #[test]
    fn test_system_prompt_empty_identity() {
        let id = Identity::default();
        assert_eq!(id.system_prompt(), "You are a helpful AI agent.");
    }

    #[test]
    fn test_system_prompt_partial() {
        let id = Identity {
            persona: Some("I am helpful.".into()),
            knowledge: Some("Project facts.".into()),
            ..Default::default()
        };

        let prompt = id.system_prompt();
        assert!(prompt.contains("# Persona\n\nI am helpful."));
        assert!(prompt.contains("# Project Knowledge\n\nProject facts."));
        assert!(!prompt.contains("# Shared Workflow"));
        assert!(!prompt.contains("# Persistent Memory"));
    }

    #[test]
    fn test_for_worker() {
        let base = tempdir().unwrap();
        let agent_dir = base.path().join("agents/worker1");
        let project_dir = base.path().join("projects/myproj");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::create_dir_all(&project_dir).unwrap();

        write_file(&agent_dir, "PERSONA.md", "I work hard.");
        write_file(&project_dir, "KNOWLEDGE.md", "The project info.");

        let id = Identity::for_worker("worker1", "myproj", base.path()).unwrap();
        assert_eq!(id.persona.as_deref(), Some("I work hard."));
        assert_eq!(id.knowledge.as_deref(), Some("The project info."));
    }

    #[test]
    fn test_is_loaded() {
        let empty = Identity::default();
        assert!(!empty.is_loaded());

        let with_persona = Identity {
            persona: Some("something".into()),
            ..Default::default()
        };
        assert!(with_persona.is_loaded());
    }
}
