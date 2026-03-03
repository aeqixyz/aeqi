use serde::{Deserialize, Serialize};

/// Lightweight project metadata stored as `project.toml` in each tenant project directory.
/// Unlike `ProjectConfig`, this has no execution fields (max_workers, execution_mode, etc.)
/// — it's purely descriptive metadata for the web API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantProjectMeta {
    pub name: String,
    pub prefix: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_toml() {
        let meta = TenantProjectMeta {
            name: "algostaking".into(),
            prefix: "as".into(),
            description: Some("HFT trading system".into()),
            repo: Some("/home/claudedev/algostaking-backend".into()),
        };
        let toml_str = toml::to_string_pretty(&meta).unwrap();
        let parsed: TenantProjectMeta = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.name, "algostaking");
        assert_eq!(parsed.prefix, "as");
        assert_eq!(parsed.repo.as_deref(), Some("/home/claudedev/algostaking-backend"));
    }

    #[test]
    fn minimal_toml() {
        let toml_str = r#"
name = "test"
prefix = "ts"
"#;
        let meta: TenantProjectMeta = toml::from_str(toml_str).unwrap();
        assert_eq!(meta.name, "test");
        assert!(meta.description.is_none());
        assert!(meta.repo.is_none());
    }
}
