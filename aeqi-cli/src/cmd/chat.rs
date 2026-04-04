use anyhow::Result;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::helpers::daemon_ipc_request;

/// Interactive chat REPL connected to the daemon MessageRouter.
pub(crate) async fn cmd_chat(config_path: &Option<PathBuf>) -> Result<()> {
    let session_id = format!(
        "cli-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let sender = std::env::var("USER").unwrap_or_else(|_| "cli".to_string());

    let status = daemon_ipc_request(config_path, &serde_json::json!({"cmd": "status"})).await;
    match &status {
        Ok(resp) if resp.get("ok").and_then(|v| v.as_bool()) == Some(true) => {
            let projects = resp
                .get("project_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let workers = resp
                .get("max_workers")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            eprintln!(
                "\n  \x1b[1maeqi\x1b[0m \x1b[32m\u{25cf}\x1b[0m {projects} projects \u{00b7} {workers} workers\n"
            );
        }
        _ => {
            eprintln!("\n  \x1b[1maeqi\x1b[0m \x1b[31m\u{25cf}\x1b[0m daemon offline");
            eprintln!("  Start with: aeqi daemon start\n");
            return Ok(());
        }
    }

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        eprint!("  \x1b[33maeqi >\x1b[0m ");
        io::stderr().flush()?;

        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break,
        };

        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        if input == "exit" || input == "quit" {
            break;
        }

        if input == "status" {
            if let Ok(resp) =
                daemon_ipc_request(config_path, &serde_json::json!({"cmd": "status"})).await
            {
                eprintln!(
                    "  {}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            }
            continue;
        }
        if input == "brief" {
            if let Ok(resp) =
                daemon_ipc_request(config_path, &serde_json::json!({"cmd": "brief"})).await
            {
                if let Some(brief) = resp.get("brief").and_then(|v| v.as_str()) {
                    eprintln!("\n{brief}\n");
                } else {
                    eprintln!("  No brief available.");
                }
            }
            continue;
        }

        eprint!("  \x1b[2m...\x1b[0m");
        io::stderr().flush()?;

        let resp = daemon_ipc_request(
            config_path,
            &serde_json::json!({
                "cmd": "chat_full",
                "message": input,
                "session_id": session_id.clone(),
                "sender": sender.clone(),
            }),
        )
        .await;

        eprint!("\r    \r");

        match resp {
            Ok(r) => {
                if let Some(ctx) = r.get("context").and_then(|v| v.as_str()) {
                    eprintln!("  \x1b[36m{ctx}\x1b[0m\n");
                } else if let Some(err) = r.get("error").and_then(|v| v.as_str()) {
                    eprintln!("  \x1b[31m{err}\x1b[0m\n");
                } else {
                    eprintln!(
                        "  {}\n",
                        serde_json::to_string_pretty(&r).unwrap_or_default()
                    );
                }

                if r.get("action").and_then(|v| v.as_str()) == Some("task_created")
                    && let Some(handle) = r.get("task_handle").and_then(|v| v.as_str())
                {
                    eprintln!("  \x1b[33m\u{27f3} Task: {handle}\x1b[0m");
                    for _ in 0..24 {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        if let Ok(poll) = daemon_ipc_request(
                            config_path,
                            &serde_json::json!({"cmd": "chat_poll", "task_id": handle}),
                        )
                        .await
                            && poll.get("completed").and_then(|v| v.as_bool()) == Some(true)
                        {
                            let text = poll.get("text").and_then(|v| v.as_str()).unwrap_or("Done.");
                            eprintln!("  \x1b[32m\u{2713} {text}\x1b[0m\n");
                            break;
                        }
                        eprint!(".");
                        io::stderr().flush()?;
                    }
                }
            }
            Err(e) => eprintln!("  \x1b[31mError: {e}\x1b[0m\n"),
        }
    }

    eprintln!();
    Ok(())
}
