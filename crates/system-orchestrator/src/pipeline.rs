use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use system_tasks::{TaskId, TaskBoard};
use std::collections::HashMap;
use std::path::Path;

/// A ritual is a workflow template: a sequence of steps with dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub ritual: RitualMeta,
    #[serde(default)]
    pub vars: HashMap<String, VarDef>,
    pub steps: Vec<PipelineStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RitualMeta {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarDef {
    pub r#type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    pub id: String,
    pub title: String,
    pub instructions: String,
    #[serde(default)]
    pub needs: Vec<String>,
}

impl Pipeline {
    /// Load a ritual from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read ritual: {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse ritual: {}", path.display()))
    }

    /// Pour (instantiate) a ritual: create a parent bead with child step beads.
    pub fn pour(
        &self,
        store: &mut TaskBoard,
        prefix: &str,
        vars: &HashMap<String, String>,
    ) -> Result<TaskId> {
        // Validate required vars.
        for (name, def) in &self.vars {
            if def.required && !vars.contains_key(name) && def.default.is_none() {
                anyhow::bail!("missing required variable: {name}");
            }
        }

        // Create parent bead.
        let parent_subject = self.interpolate(&self.ritual.description, vars);
        let parent = store.create(prefix, &parent_subject)?;

        // Create step beads as children.
        let mut step_quest_ids: HashMap<String, TaskId> = HashMap::new();

        for step in &self.steps {
            let subject = self.interpolate(&step.title, vars);
            let child = store.create_child(&parent.id, &subject)?;

            // Set description with interpolated instructions.
            let description = self.interpolate(&step.instructions, vars);
            store.update(&child.id.0, |b| {
                b.description = description;
            })?;

            step_quest_ids.insert(step.id.clone(), child.id.clone());
        }

        // Wire up dependencies based on `needs`.
        for step in &self.steps {
            if let Some(step_quest_id) = step_quest_ids.get(&step.id) {
                for need in &step.needs {
                    if let Some(dep_id) = step_quest_ids.get(need) {
                        store.add_dependency(&step_quest_id.0, &dep_id.0)?;
                    }
                }
            }
        }

        Ok(parent.id)
    }

    /// Simple variable interpolation: replace {{var_name}} with values.
    fn interpolate(&self, template: &str, vars: &HashMap<String, String>) -> String {
        let mut result = template.to_string();
        for (key, value) in vars {
            result = result.replace(&format!("{{{{{key}}}}}"), value);
        }
        result
    }
}
