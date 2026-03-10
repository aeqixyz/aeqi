use anyhow::{Context, Result};
use sigil_orchestrator::Daemon;
use std::path::PathBuf;

use crate::cli::ConfigAction;
use crate::helpers::{load_config, pid_file_path};

pub(crate) async fn cmd_config(config_path: &Option<PathBuf>, action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => {
            let (config, path) = load_config(config_path)?;
            println!("Config: {}\n", path.display());
            println!("Name: {}", config.sigil.name);
            println!("Data dir: {}", config.data_dir().display());

            if let Some(ref or) = config.providers.openrouter {
                println!("\n[providers.openrouter]");
                println!("  default_model: {}", or.default_model);
                println!(
                    "  fallback_model: {}",
                    or.fallback_model.as_deref().unwrap_or("(none)")
                );
                println!(
                    "  api_key: {}...",
                    if or.api_key.len() > 8 {
                        &or.api_key[..8]
                    } else {
                        "***"
                    }
                );
            }

            println!("\n[security]");
            println!("  autonomy: {:?}", config.security.autonomy);
            println!("  workspace_only: {}", config.security.workspace_only);
            println!(
                "  max_cost_per_day_usd: {}",
                config.security.max_cost_per_day_usd
            );

            println!("\n[heartbeat]");
            println!("  enabled: {}", config.heartbeat.enabled);
            println!(
                "  interval: {}min",
                config.heartbeat.default_interval_minutes
            );

            println!("\n[[projects]]");
            for proj in &config.projects {
                println!(
                    "  {} prefix={} model={} workers={}",
                    proj.name,
                    proj.prefix,
                    proj.model.as_deref().unwrap_or("default"),
                    proj.max_workers
                );
            }
        }

        ConfigAction::Reload => {
            let (config, _) = load_config(config_path)?;
            let pid_path = pid_file_path(&config);

            if !Daemon::is_running_from_pid(&pid_path) {
                println!("No daemon running. Config will be loaded on next `sigil daemon start`.");
                return Ok(());
            }

            // Send SIGHUP to the daemon process.
            #[cfg(unix)]
            {
                let pid_str = std::fs::read_to_string(&pid_path)?;
                let pid: u32 = pid_str.trim().parse().context("invalid PID file")?;

                use std::process::Command;
                let status = Command::new("kill")
                    .args(["-HUP", &pid.to_string()])
                    .status()?;
                if status.success() {
                    println!("Sent SIGHUP to daemon (PID {pid}). Config will be reloaded.");
                } else {
                    println!("Failed to send SIGHUP to daemon (PID {pid}).");
                }
            }
            #[cfg(not(unix))]
            {
                println!("Config reload not supported on this platform. Restart the daemon.");
            }
        }
    }
    Ok(())
}
