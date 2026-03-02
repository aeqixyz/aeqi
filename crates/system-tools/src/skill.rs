use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A skill is a TOML-defined reusable capability — prompt template + tool allowlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub skill: SkillMeta,
    #[serde(default)]
    pub tools: MagicTools,
    pub prompt: SkillPrompt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub triggers: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MagicTools {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPrompt {
    pub system: String,
    #[serde(default)]
    pub user_prefix: String,
}

impl Skill {
    /// Load a skill from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read skill: {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse skill: {}", path.display()))
    }

    /// Discover all skills in a directory.
    pub fn discover(dir: &Path) -> Result<Vec<Self>> {
        let mut skills = Vec::new();
        if !dir.exists() {
            return Ok(skills);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "toml") {
                match Self::load(&path) {
                    Ok(skill) => skills.push(skill),
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "skipping invalid skill");
                    }
                }
            }
        }
        skills.sort_by(|a, b| a.skill.name.cmp(&b.skill.name));
        Ok(skills)
    }

    /// Build the full system prompt for this skill.
    pub fn system_prompt(&self, base_identity: &str) -> String {
        if base_identity.is_empty() {
            self.prompt.system.clone()
        } else {
            format!("{}\n\n---\n\n# Skill: {}\n\n{}", base_identity, self.skill.name, self.prompt.system)
        }
    }

    /// Check if a tool is allowed by this skill's policy.
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        if !self.tools.deny.is_empty() && self.tools.deny.contains(&tool_name.to_string()) {
            return false;
        }
        if !self.tools.allow.is_empty() {
            return self.tools.allow.contains(&tool_name.to_string());
        }
        true // If no allow/deny lists, everything is allowed.
    }
}
