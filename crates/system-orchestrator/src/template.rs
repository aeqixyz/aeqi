//! Template — Reusable workflow templates that generate quest DAGs.
//!
//! A Template is a TOML template defining a sequence of steps with variable
//! substitution. When "poured", it creates a parent quest with child quests
//! for each step, linked by dependency chains.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub variables: Vec<TemplateVariable>,
    pub steps: Vec<TemplateStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVariable {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateStep {
    pub name: String,
    pub description: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default = "default_true")]
    pub sequential: bool,
}

fn default_priority() -> String {
    "normal".to_string()
}
fn default_true() -> bool {
    true
}

#[derive(Debug)]
pub struct PouredFormula {
    pub parent_subject: String,
    pub parent_description: String,
    pub steps: Vec<PouredStep>,
}

#[derive(Debug)]
pub struct PouredStep {
    pub subject: String,
    pub description: String,
    pub priority: String,
    pub acceptance_criteria: Option<String>,
    pub labels: Vec<String>,
    pub depends_on_previous: bool,
}

impl Template {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read formula: {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse formula: {}", path.display()))
    }

    pub fn load_all(dir: &Path) -> Result<Vec<Self>> {
        let mut formulas = Vec::new();
        if !dir.exists() {
            return Ok(formulas);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "toml") {
                match Self::load(&path) {
                    Ok(f) => formulas.push(f),
                    Err(e) => tracing::warn!(path = %path.display(), error = %e, "skipping formula"),
                }
            }
        }
        Ok(formulas)
    }

    /// Pour: resolve variables and produce quest structure.
    pub fn pour(&self, vars: &HashMap<String, String>) -> Result<PouredFormula> {
        for v in &self.variables {
            if v.required && !vars.contains_key(&v.name) && v.default.is_none() {
                anyhow::bail!("required variable '{}' not provided", v.name);
            }
        }

        let mut resolved: HashMap<String, String> = HashMap::new();
        for v in &self.variables {
            let value = vars
                .get(&v.name)
                .cloned()
                .or_else(|| v.default.clone())
                .unwrap_or_default();
            resolved.insert(v.name.clone(), value);
        }

        let substitute = |text: &str| -> String {
            let mut result = text.to_string();
            for (k, v) in &resolved {
                result = result.replace(&format!("{{{{{k}}}}}"), v);
            }
            result
        };

        let steps = self
            .steps
            .iter()
            .map(|step| PouredStep {
                subject: substitute(&step.name),
                description: substitute(&step.description),
                priority: step.priority.clone(),
                acceptance_criteria: step.acceptance_criteria.as_ref().map(|c| substitute(c)),
                labels: step.labels.iter().map(|l| substitute(l)).collect(),
                depends_on_previous: step.sequential,
            })
            .collect();

        Ok(PouredFormula {
            parent_subject: substitute(&self.name),
            parent_description: substitute(&self.description),
            steps,
        })
    }
}

/// Discover formulas from shared + project directories (project overrides shared).
pub fn discover_formulas(shared_dir: &Path, project_dir: &Path) -> Result<Vec<Template>> {
    let mut formulas = Vec::new();

    let shared_mol = shared_dir.join("molecules");
    if shared_mol.exists() {
        formulas.extend(Template::load_all(&shared_mol)?);
    }

    let project_mol = project_dir.join("molecules");
    if project_mol.exists() {
        for df in Template::load_all(&project_mol)? {
            if let Some(pos) = formulas.iter().position(|f| f.name == df.name) {
                formulas[pos] = df;
            } else {
                formulas.push(df);
            }
        }
    }

    Ok(formulas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formula_parse_and_pour() {
        let toml_str = r#"
name = "Feature: {{feature_name}}"
description = "Implement {{feature_name}} in {{repo}}"

[[variables]]
name = "feature_name"
description = "Name of the feature"
required = true

[[variables]]
name = "repo"
description = "Target repository"
default = "algostaking-backend"

[[steps]]
name = "Research {{feature_name}}"
description = "Explore codebase for {{feature_name}}"
priority = "high"
labels = ["research"]
sequential = false

[[steps]]
name = "Implement {{feature_name}}"
description = "Write code for {{feature_name}} in {{repo}}"
priority = "high"
acceptance_criteria = "Tests pass, code compiles"
labels = ["development"]

[[steps]]
name = "Review {{feature_name}}"
description = "Review implementation"
priority = "normal"
labels = ["review"]
"#;
        let formula: Template = toml::from_str(toml_str).unwrap();
        assert_eq!(formula.steps.len(), 3);

        let mut vars = HashMap::new();
        vars.insert("feature_name".into(), "WebSocket".into());
        let poured = formula.pour(&vars).unwrap();
        assert_eq!(poured.parent_subject, "Feature: WebSocket");
        assert_eq!(poured.steps[0].subject, "Research WebSocket");
        assert!(poured.steps[1].description.contains("algostaking-backend"));
    }

    #[test]
    fn test_missing_required() {
        let toml_str = r#"
name = "test"
description = "test"

[[variables]]
name = "required_var"
description = "must provide"
required = true

[[steps]]
name = "do it"
description = "{{required_var}}"
"#;
        let formula: Template = toml::from_str(toml_str).unwrap();
        assert!(formula.pour(&HashMap::new()).is_err());
    }
}
