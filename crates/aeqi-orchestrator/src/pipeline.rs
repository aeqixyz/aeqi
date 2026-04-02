use aeqi_tasks::{TaskBoard, TaskId};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A pipeline is a workflow template: a sequence of steps with dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    #[serde(alias = "ritual")]
    pub meta: PipelineMeta,
    #[serde(default)]
    pub vars: HashMap<String, VarDef>,
    pub steps: Vec<PipelineStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineMeta {
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
    /// Load a pipeline from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read pipeline: {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse pipeline: {}", path.display()))
    }

    /// Instantiate a pipeline: create a parent task with child step tasks.
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

        // Create parent task.
        let parent_subject = self.interpolate(&self.meta.description, vars);
        let parent = store.create_with_agent(prefix, &parent_subject, None)?;

        // Create step tasks as children.
        let mut step_task_ids: HashMap<String, TaskId> = HashMap::new();

        for step in &self.steps {
            let subject = self.interpolate(&step.title, vars);
            let child = store.create_child(&parent.id, &subject)?;

            // Set description with interpolated instructions.
            let description = self.interpolate(&step.instructions, vars);
            store.update(&child.id.0, |b| {
                b.description = description;
            })?;

            step_task_ids.insert(step.id.clone(), child.id.clone());
        }

        // Wire up dependencies based on `needs`.
        for step in &self.steps {
            if let Some(step_task_id) = step_task_ids.get(&step.id) {
                for need in &step.needs {
                    if let Some(dep_id) = step_task_ids.get(need) {
                        store.add_dependency(&step_task_id.0, &dep_id.0)?;
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
