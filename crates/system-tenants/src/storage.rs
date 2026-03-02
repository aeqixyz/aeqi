use anyhow::{Context, Result};
use std::path::Path;

use crate::tenant::TenantMeta;

/// Load tenant metadata from tenant.toml.
pub fn load_tenant_meta(data_dir: &Path) -> Result<TenantMeta> {
    let path = data_dir.join("tenant.toml");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read tenant.toml: {}", path.display()))?;
    let meta: TenantMeta = toml::from_str(&content)
        .with_context(|| "failed to parse tenant.toml")?;
    Ok(meta)
}

/// Calculate disk usage of a tenant's data directory in bytes.
pub fn disk_usage(data_dir: &Path) -> u64 {
    walkdir(data_dir)
}

fn walkdir(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += walkdir(&path);
            } else if let Ok(meta) = path.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// Disk usage in megabytes.
pub fn disk_usage_mb(data_dir: &Path) -> f64 {
    disk_usage(data_dir) as f64 / (1024.0 * 1024.0)
}
