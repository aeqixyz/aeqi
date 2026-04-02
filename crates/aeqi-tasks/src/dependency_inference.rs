//! Semantic Task Dependency Inference.
//!
//! Pure computation — no LLM, no DB. Extracts entities (file paths, function
//! names, module names) from task subjects and descriptions, then computes
//! overlap to suggest implicit dependencies.

use crate::task::{Task, TaskId};
use serde::{Deserialize, Serialize};

/// An inferred dependency between two tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredDependency {
    pub from: TaskId,
    pub to: TaskId,
    pub reason: String,
    pub confidence: f64,
}

/// Infer dependencies between tasks based on entity overlap in subjects/descriptions.
/// Returns suggested dependencies above the confidence threshold.
pub fn infer_dependencies(tasks: &[&Task], threshold: f64) -> Vec<InferredDependency> {
    let mut deps = Vec::new();

    // Extract entities for each task.
    let entities: Vec<Vec<String>> = tasks
        .iter()
        .map(|t| extract_entities(&t.subject, &t.description))
        .collect();

    // Compare all pairs.
    for i in 0..tasks.len() {
        for j in (i + 1)..tasks.len() {
            if let Some((confidence, reason)) = entity_overlap(&entities[i], &entities[j])
                && confidence >= threshold
            {
                let (from, to) = determine_direction(tasks[i], tasks[j]);
                deps.push(InferredDependency {
                    from,
                    to,
                    reason,
                    confidence,
                });
            }
        }
    }

    deps
}

/// Extract meaningful entities from subject and description.
/// Looks for: file paths, snake_case identifiers, PascalCase identifiers, module names.
fn extract_entities(subject: &str, description: &str) -> Vec<String> {
    let combined = format!("{subject} {description}");
    let mut entities = Vec::new();

    for word in
        combined.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')')
    {
        let word =
            word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.' && c != '/');
        if word.is_empty() || word.len() < 3 {
            continue;
        }

        // File paths: contains / or ends with common extensions
        if word.contains('/')
            || word.ends_with(".rs")
            || word.ends_with(".ts")
            || word.ends_with(".py")
            || word.ends_with(".js")
            || word.ends_with(".toml")
            || word.ends_with(".json")
        {
            entities.push(word.to_lowercase());
            continue;
        }

        // snake_case identifiers
        if word.contains('_') && word.chars().all(|c| c.is_alphanumeric() || c == '_') {
            entities.push(word.to_lowercase());
            continue;
        }

        // PascalCase / camelCase identifiers (has mixed case, no spaces)
        if word.chars().any(|c| c.is_uppercase())
            && word.chars().any(|c| c.is_lowercase())
            && word.chars().all(|c| c.is_alphanumeric())
            && word.len() >= 4
        {
            entities.push(word.to_lowercase());
            continue;
        }
    }

    entities.sort();
    entities.dedup();
    entities
}

/// Compute entity overlap between two entity sets.
/// Returns (confidence, reason) if there's meaningful overlap.
fn entity_overlap(a: &[String], b: &[String]) -> Option<(f64, String)> {
    if a.is_empty() || b.is_empty() {
        return None;
    }

    let shared: Vec<&String> = a.iter().filter(|e| b.contains(e)).collect();
    if shared.is_empty() {
        return None;
    }

    let total = (a.len() + b.len()) as f64 / 2.0;
    let confidence = shared.len() as f64 / total;
    let reason = format!(
        "shared entities: {}",
        shared
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    Some((confidence.min(1.0), reason))
}

/// Determine dependency direction between two tasks.
/// Heuristic: "create/setup/init" tasks come before "test/verify/deploy" tasks.
fn determine_direction(a: &Task, b: &Task) -> (TaskId, TaskId) {
    let create_words = [
        "create",
        "setup",
        "init",
        "add",
        "implement",
        "build",
        "define",
        "configure",
    ];
    let test_words = [
        "test", "verify", "validate", "check", "deploy", "review", "document",
    ];

    let a_subject = a.subject.to_lowercase();
    let b_subject = b.subject.to_lowercase();

    let a_is_create = create_words.iter().any(|w| a_subject.contains(w));
    let b_is_test = test_words.iter().any(|w| b_subject.contains(w));
    let b_is_create = create_words.iter().any(|w| b_subject.contains(w));
    let a_is_test = test_words.iter().any(|w| a_subject.contains(w));

    if a_is_create && b_is_test {
        // b depends on a (create before test)
        (b.id.clone(), a.id.clone())
    } else if b_is_create && a_is_test {
        // a depends on b
        (a.id.clone(), b.id.clone())
    } else {
        // Default: earlier created task is the dependency
        if a.created_at <= b.created_at {
            (b.id.clone(), a.id.clone())
        } else {
            (a.id.clone(), b.id.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Priority, Task, TaskId, TaskStatus};

    fn make_task(id: &str, subject: &str, description: &str) -> Task {
        Task {
            id: TaskId(id.to_string()),
            subject: subject.to_string(),
            description: description.to_string(),
            status: TaskStatus::Pending,
            priority: Priority::Normal,
            assignee: None,
            agent_id: None,
            depends_on: vec![],
            blocks: vec![],
            skill: None,
            labels: vec![],
            retry_count: 0,
            checkpoints: vec![],
            metadata: serde_json::Value::Null,
            created_at: chrono::Utc::now(),
            updated_at: None,
            closed_at: None,
            closed_reason: None,
            acceptance_criteria: None,
            locked_by: None,
            locked_at: None,
        }
    }

    #[test]
    fn test_extract_entities_file_paths() {
        let entities = extract_entities(
            "Fix bug in src/auth.rs",
            "The auth module at src/auth.rs has issues",
        );
        assert!(entities.contains(&"src/auth.rs".to_string()));
    }

    #[test]
    fn test_extract_entities_function_names() {
        let entities = extract_entities(
            "Refactor validate_token",
            "Update the validate_token function",
        );
        assert!(entities.contains(&"validate_token".to_string()));
    }

    #[test]
    fn test_overlap_same_file() {
        let t1 = make_task("t-001", "Fix bug in src/auth.rs", "Authentication issue");
        let t2 = make_task("t-002", "Add tests for src/auth.rs", "Test the auth module");

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let deps = infer_dependencies(&tasks, 0.1);

        assert!(!deps.is_empty());
        assert!(deps[0].reason.contains("src/auth.rs"));
    }

    #[test]
    fn test_direction_create_before_test() {
        let t1 = make_task("t-001", "Create user authentication", "Build auth system");
        let t2 = make_task("t-002", "Test user authentication", "Verify auth works");

        let (from, to) = determine_direction(&t1, &t2);
        // t2 (test) depends on t1 (create)
        assert_eq!(from, t2.id);
        assert_eq!(to, t1.id);
    }

    #[test]
    fn test_no_cycle_on_apply() {
        // Ensure inferred deps don't create cycles (they're directional by creation order)
        let t1 = make_task("t-001", "Setup database", "Init DB schema");
        let t2 = make_task("t-002", "Test database", "Verify DB operations");

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let deps = infer_dependencies(&tasks, 0.1);

        // Should not have both directions
        for dep in &deps {
            let reverse_exists = deps.iter().any(|d| d.from == dep.to && d.to == dep.from);
            assert!(
                !reverse_exists,
                "bidirectional dependency detected (would create cycle)"
            );
        }
    }

    #[test]
    fn test_no_deps_between_unrelated() {
        let t1 = make_task("t-001", "Fix login page CSS", "Adjust button colors");
        let t2 = make_task(
            "t-002",
            "Optimize database queries",
            "Add indexes to users table",
        );

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let deps = infer_dependencies(&tasks, 0.3);

        assert!(deps.is_empty());
    }
}
