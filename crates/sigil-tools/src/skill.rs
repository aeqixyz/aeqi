use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A skill is a TOML-defined reusable capability — prompt template + tool allowlist.
///
/// Skills combine what CC calls "skills" (prompt + tool restrictions) with
/// execution metadata (context, arguments, auto-invocation triggers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub skill: SkillMeta,
    #[serde(default)]
    pub tools: MagicTools,
    pub prompt: SkillPrompt,
    /// Optional verification commands and expected output patterns.
    pub verification: Option<SkillVerification>,
    /// Optional execution configuration (parallelism, worktree isolation).
    pub execution: Option<SkillExecution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    /// When the model should auto-invoke this skill. Describes trigger conditions
    /// in natural language. Example: "Use when the user wants to cherry-pick a PR."
    #[serde(default)]
    pub when_to_use: Option<String>,
    /// Execution context: "fork" runs as subagent with separate context,
    /// "inline" (default) expands into the current conversation.
    #[serde(default)]
    pub context: Option<String>,
    /// Named arguments the skill accepts. Use `$arg_name` in prompt for substitution.
    #[serde(default)]
    pub arguments: Vec<String>,
    /// Hint showing argument placeholders (e.g., "<pr_number> <target_branch>").
    #[serde(default)]
    pub argument_hint: Option<String>,
    /// Model override for this skill. If set, the worker uses this model
    /// instead of its default. Useful for using cheaper models on reflective
    /// tasks or stronger models on complex analysis.
    #[serde(default)]
    pub model: Option<String>,
    /// Allow shell command execution in prompt template (`!` backtick syntax).
    /// Default: false. When true, `!`backtick`` blocks in the system prompt
    /// are executed at load time and replaced with their stdout.
    #[serde(default)]
    pub allow_shell_in_prompt: bool,
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

/// Verification commands and expected output patterns for a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVerification {
    /// Commands to run for verification (e.g., `["cargo test"]`).
    #[serde(default)]
    pub commands: Vec<String>,
    /// Patterns expected in the command output (e.g., `["0 failed"]`).
    #[serde(default)]
    pub expected_patterns: Vec<String>,
}

/// Execution configuration for skills that orchestrate parallel work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExecution {
    /// Execution mode: "parallel" (fan-out to multiple agents) or "sequential" (default).
    #[serde(default = "default_execution_mode")]
    pub mode: String,
    /// Number of parallel agents for fan-out mode. Default: 1.
    #[serde(default = "default_agent_count")]
    pub agents: u32,
    /// Whether each agent runs in an isolated git worktree. Default: false.
    #[serde(default)]
    pub worktree: bool,
    /// Maximum budget per agent execution (USD). Default: no limit.
    pub max_budget_usd: Option<f64>,
}

fn default_execution_mode() -> String {
    "sequential".to_string()
}

fn default_agent_count() -> u32 {
    1
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
    /// If `allow_shell_in_prompt` is true, executes `!` backtick blocks and
    /// replaces them with their stdout.
    pub fn system_prompt(&self, base_identity: &str) -> String {
        let mut prompt = self.prompt.system.clone();

        // Expand shell commands in prompt if enabled.
        if self.skill.allow_shell_in_prompt {
            prompt = Self::expand_shell_commands(&prompt);
        }

        if base_identity.is_empty() {
            prompt
        } else {
            format!(
                "{}\n\n---\n\n# Skill: {}\n\n{}",
                base_identity, self.skill.name, prompt
            )
        }
    }

    /// Execute `!` backtick blocks in a prompt string and replace with stdout.
    /// Format: !`command here` — the backtick content is passed to bash -c.
    /// On error, the block is replaced with an error message.
    fn expand_shell_commands(prompt: &str) -> String {
        let mut result = String::with_capacity(prompt.len());
        let mut remaining = prompt;

        while let Some(start) = remaining.find("!`") {
            result.push_str(&remaining[..start]);
            let after_marker = &remaining[start + 2..];

            if let Some(end) = after_marker.find('`') {
                let command = &after_marker[..end];
                let output = std::process::Command::new("bash")
                    .arg("-c")
                    .arg(command)
                    .output();

                match output {
                    Ok(out) if out.status.success() => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        result.push_str(stdout.trim_end());
                    }
                    Ok(out) => {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        result.push_str(&format!("[shell error: {}]", stderr.trim()));
                    }
                    Err(e) => {
                        result.push_str(&format!("[shell exec failed: {e}]"));
                    }
                }

                remaining = &after_marker[end + 1..];
            } else {
                // Unclosed backtick — keep as-is.
                result.push_str("!`");
                remaining = after_marker;
            }
        }
        result.push_str(remaining);
        result
    }

    /// Whether this skill should run as a forked subagent (separate context).
    pub fn is_fork_context(&self) -> bool {
        self.skill
            .context
            .as_deref()
            .is_some_and(|c| c == "fork")
    }

    /// Whether this skill has auto-invocation criteria.
    pub fn has_auto_trigger(&self) -> bool {
        self.skill.when_to_use.is_some()
    }

    /// Substitute `$arg_name` placeholders in the prompt with actual argument values.
    pub fn substitute_args(&self, args: &std::collections::HashMap<String, String>) -> String {
        let mut prompt = self.prompt.system.clone();
        for (key, value) in args {
            prompt = prompt.replace(&format!("${key}"), value);
        }
        prompt
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_with_new_fields() {
        let toml = r#"
[skill]
name = "deploy"
description = "Deploy a service"
phase = "workflow"
when_to_use = "Use when the user wants to deploy to production"
context = "fork"
arguments = ["service", "env"]
argument_hint = "<service> <env>"

[tools]
allow = ["shell", "read_file"]

[prompt]
system = "Deploy $service to $env"

[execution]
mode = "parallel"
agents = 3
worktree = true
max_budget_usd = 1.50
"#;
        let skill: Skill = toml::from_str(toml).unwrap();
        assert_eq!(skill.skill.name, "deploy");
        assert_eq!(
            skill.skill.when_to_use.as_deref(),
            Some("Use when the user wants to deploy to production")
        );
        assert_eq!(skill.skill.context.as_deref(), Some("fork"));
        assert!(skill.is_fork_context());
        assert!(skill.has_auto_trigger());
        assert_eq!(skill.skill.arguments, vec!["service", "env"]);
        assert_eq!(skill.skill.argument_hint.as_deref(), Some("<service> <env>"));

        let exec = skill.execution.unwrap();
        assert_eq!(exec.mode, "parallel");
        assert_eq!(exec.agents, 3);
        assert!(exec.worktree);
        assert!((exec.max_budget_usd.unwrap() - 1.50).abs() < f64::EPSILON);
    }

    #[test]
    fn test_backward_compatible_existing_skills() {
        // Existing skills without new fields should still parse.
        let toml = r#"
[skill]
name = "health-check"
description = "Check health"
phase = "autonomous"

[tools]
allow = ["shell"]

[prompt]
system = "Check health"
"#;
        let skill: Skill = toml::from_str(toml).unwrap();
        assert_eq!(skill.skill.name, "health-check");
        assert!(!skill.is_fork_context());
        assert!(!skill.has_auto_trigger());
        assert!(skill.skill.arguments.is_empty());
        assert!(skill.execution.is_none());
    }

    #[test]
    fn test_argument_substitution() {
        let toml = r#"
[skill]
name = "test"
description = "test"
arguments = ["name", "target"]

[prompt]
system = "Deploy $name to $target environment"
"#;
        let skill: Skill = toml::from_str(toml).unwrap();
        let mut args = std::collections::HashMap::new();
        args.insert("name".to_string(), "myapp".to_string());
        args.insert("target".to_string(), "production".to_string());

        let result = skill.substitute_args(&args);
        assert_eq!(result, "Deploy myapp to production environment");
    }

    #[test]
    fn test_shell_expansion_in_prompt() {
        let toml = r#"
[skill]
name = "test"
description = "test"
allow_shell_in_prompt = true

[prompt]
system = "Date: !`echo 2026-04-01` and host: !`echo testhost`"
"#;
        let skill: Skill = toml::from_str(toml).unwrap();
        let prompt = skill.system_prompt("");
        assert!(prompt.contains("2026-04-01"), "got: {prompt}");
        assert!(prompt.contains("testhost"), "got: {prompt}");
        assert!(!prompt.contains("!`"), "shell markers should be replaced");
    }

    #[test]
    fn test_shell_expansion_disabled_by_default() {
        let toml = r#"
[skill]
name = "test"
description = "test"

[prompt]
system = "Should not expand: !`echo danger`"
"#;
        let skill: Skill = toml::from_str(toml).unwrap();
        let prompt = skill.system_prompt("");
        // Shell blocks NOT expanded when allow_shell_in_prompt is false.
        assert!(prompt.contains("!`echo danger`"));
    }

    #[test]
    fn test_model_override() {
        let toml = r#"
[skill]
name = "cheap-task"
description = "Uses a cheaper model"
model = "anthropic/claude-haiku-4-5"

[prompt]
system = "Do something cheap"
"#;
        let skill: Skill = toml::from_str(toml).unwrap();
        assert_eq!(
            skill.skill.model.as_deref(),
            Some("anthropic/claude-haiku-4-5")
        );
    }

    #[test]
    fn test_tool_allowed() {
        let toml = r#"
[skill]
name = "test"
description = "test"

[tools]
allow = ["shell", "read_file"]
deny = ["write_file"]

[prompt]
system = "test"
"#;
        let skill: Skill = toml::from_str(toml).unwrap();
        assert!(skill.is_tool_allowed("shell"));
        assert!(skill.is_tool_allowed("read_file"));
        assert!(!skill.is_tool_allowed("write_file"));
        assert!(!skill.is_tool_allowed("edit_file")); // Not in allow list
    }
}
