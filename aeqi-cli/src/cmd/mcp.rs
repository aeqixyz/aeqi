use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::Instant;

use crate::helpers::load_config;

#[derive(Debug, Deserialize)]
struct McpRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct McpResponse {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct ToolDef {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: serde_json::Value,
}

fn ipc_request_sync(
    data_dir: &std::path::Path,
    request: &serde_json::Value,
) -> Result<serde_json::Value> {
    let sock_path = data_dir.join("rm.sock");
    let stream = std::os::unix::net::UnixStream::connect(&sock_path)?;
    let mut writer = io::BufWriter::new(&stream);
    let mut reader = io::BufReader::new(&stream);

    let mut req_bytes = serde_json::to_vec(request)?;
    req_bytes.push(b'\n');
    writer.write_all(&req_bytes)?;
    writer.flush()?;

    let mut line = String::new();
    reader.read_line(&mut line)?;
    let response: serde_json::Value = serde_json::from_str(&line)?;
    Ok(response)
}

fn extract_frontmatter_field(content: &str, field: &str) -> Option<String> {
    let mut in_frontmatter = false;
    for line in content.lines() {
        if line.trim() == "---" {
            if in_frontmatter {
                return None;
            }
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            let prefix = format!("{field}: ");
            if let Some(val) = line.strip_prefix(&prefix) {
                return Some(val.trim().to_string());
            }
        }
    }
    None
}

fn scan_dir(dir: &std::path::Path, source: &str) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    if !dir.exists() {
        return items;
    }
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "toml" && ext != "md" {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let description = extract_frontmatter_field(&content, "description").unwrap_or_default();
        let phase = extract_frontmatter_field(&content, "phase").unwrap_or_default();
        let model = extract_frontmatter_field(&content, "model").unwrap_or_default();
        let preview: String = if description.is_empty() {
            content
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(120)
                .collect()
        } else {
            description.chars().take(120).collect()
        };
        items.push(serde_json::json!({
            "name": name,
            "source": source,
            "kind": if ext == "toml" { "skill" } else { "doc" },
            "preview": preview,
            "phase": phase,
            "model": model,
            "content": content,
        }));
    }
    items
}

pub fn cmd_mcp(config_path: &Option<PathBuf>) -> Result<()> {
    let (config, config_file) = load_config(config_path)?;
    let data_dir = config.data_dir();
    let base_dir = config_file
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let tools = vec![
        ToolDef {
            name: "aeqi_projects".to_string(),
            description: "List all AEQI projects with repo paths, prefixes, and teams. Use to discover project names and match working directories.".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolDef {
            name: "aeqi_primer".to_string(),
            description: "Get a project's primer context (AEQI.md) — architecture, critical rules, build/deploy. This is the essential project brief. Call this before starting work on any project.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": {"type": "string", "description": "Project name"}
                },
                "required": ["project"]
            }),
        },
        ToolDef {
            name: "aeqi_skills".to_string(),
            description: "List or retrieve skills — domain knowledge, procedures, and checklists. Filter by phase to get phase-relevant knowledge.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["list", "get"], "default": "list"},
                    "project": {"type": "string", "description": "Filter by project (optional)"},
                    "phase": {"type": "string", "enum": ["discover", "plan", "implement", "verify", "finalize", "workflow"], "description": "Filter by pipeline phase (optional)"},
                    "name": {"type": "string", "description": "Skill name (required for get)"}
                }
            }),
        },
        ToolDef {
            name: "aeqi_agents".to_string(),
            description: "List or retrieve agent definitions — autonomous actor templates with specialized prompts. Filter by pipeline phase (discover/plan/implement/verify/finalize) to get only relevant agents.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["list", "get"], "default": "list"},
                    "project": {"type": "string", "description": "Filter by project (optional)"},
                    "phase": {"type": "string", "enum": ["discover", "plan", "implement", "verify", "finalize", "workflow"], "description": "Filter by pipeline phase (optional)"},
                    "name": {"type": "string", "description": "Agent name (required for get)"}
                }
            }),
        },
        ToolDef {
            name: "aeqi_recall".to_string(),
            description: "Search memory for relevant knowledge. Searches within a project's memory by default, or across all projects with scope 'system'.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": {"type": "string", "description": "Project to search"},
                    "query": {"type": "string", "description": "Natural language query"},
                    "limit": {"type": "integer", "description": "Max results", "default": 5},
                    "scope": {"type": "string", "enum": ["domain", "system", "entity"], "default": "domain", "description": "domain = project-level, system = cross-project, entity = per-agent"}
                },
                "required": ["project", "query"]
            }),
        },
        ToolDef {
            name: "aeqi_remember".to_string(),
            description: "Store knowledge in memory for future recall.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": {"type": "string", "description": "Project this belongs to"},
                    "key": {"type": "string", "description": "Short slug key"},
                    "content": {"type": "string", "description": "The knowledge to store"},
                    "category": {"type": "string", "enum": ["fact", "procedure", "preference", "context", "evergreen"], "default": "fact"},
                    "scope": {"type": "string", "enum": ["domain", "system", "entity"], "default": "domain", "description": "domain = project-level, system = cross-project, entity = per-agent"},
                    "entity_id": {"type": "string", "description": "Agent name (required when scope is 'entity')"}
                },
                "required": ["project", "key", "content"]
            }),
        },
        ToolDef {
            name: "aeqi_status".to_string(),
            description: "Live status: active workers, budget, costs, pending tasks.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": {"type": "string", "description": "Filter to project (optional)"}
                }
            }),
        },
        ToolDef {
            name: "aeqi_notes".to_string(),
            description: "Shared coordination surface. Post discoveries, claim resources, signal state, query entries, and coordinate across agents and projects.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["read", "post", "get", "query", "claim", "release", "delete"],
                        "description": "read: list all entries. post: create entry. get: lookup by key. query: filter by tags. claim: exclusive resource lock. release: drop claim. delete: remove entry."
                    },
                    "project": {"type": "string"},
                    "key": {"type": "string", "description": "Entry key (post/get/delete)"},
                    "resource": {"type": "string", "description": "Resource to claim/release (e.g. file path)"},
                    "content": {"type": "string", "description": "Entry content (post/claim)"},
                    "tags": {"type": "array", "items": {"type": "string"}, "description": "Tags for filtering (post/query)"},
                    "prefix": {"type": "string", "description": "Filter entries by key prefix (read/query). E.g. 'task:abc' returns all task:abc:* entries."},
                    "durability": {"type": "string", "enum": ["transient", "durable"], "description": "TTL class (default: transient=24h, durable=7d)"},
                    "since": {"type": "string", "description": "ISO 8601 timestamp — only return entries created after this (read/query)"},
                    "cross_project": {"type": "boolean", "description": "Search across all projects (read/query)"},
                    "limit": {"type": "integer", "description": "Max results (read/query, default: 20)"},
                    "force": {"type": "boolean", "description": "Force release even if claimed by another agent"}
                },
                "required": ["action", "project"]
            }),
        },
        ToolDef {
            name: "aeqi_create_task".to_string(),
            description: "Create a task in a AEQI project for the team to execute.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": {"type": "string"},
                    "subject": {"type": "string", "description": "Short task title"},
                    "description": {"type": "string", "description": "Detailed description (optional)"}
                },
                "required": ["project", "subject"]
            }),
        },
        ToolDef {
            name: "aeqi_close_task".to_string(),
            description: "Close/complete a task by ID.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": {"type": "string"},
                    "reason": {"type": "string"}
                },
                "required": ["task_id"]
            }),
        },
        ToolDef {
            name: "aeqi_graph".to_string(),
            description: "Query the code intelligence graph. Search symbols, get 360° context (callers/callees/implementors), analyze blast radius of changes, list communities or processes.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["search", "context", "impact", "file", "stats", "index"], "description": "search=FTS symbol search, context=360° view of a symbol, impact=blast radius, file=symbols in a file, stats=graph statistics, index=re-index project"},
                    "project": {"type": "string", "description": "Project name"},
                    "query": {"type": "string", "description": "Search query (for search action)"},
                    "node_id": {"type": "string", "description": "Node ID (for context/impact actions)"},
                    "file_path": {"type": "string", "description": "File path relative to project root (for file action)"},
                    "depth": {"type": "integer", "description": "Max traversal depth (impact, default 3)", "default": 3},
                    "limit": {"type": "integer", "description": "Max results (default 10)", "default": 10}
                },
                "required": ["action", "project"]
            }),
        },
        ToolDef {
            name: "aeqi_delegate".to_string(),
            description: "Delegate work to a AEQI agent. Loads the agent template, gathers task context from notes, and returns a structured prompt ready to pass to a Claude Code subagent. One call replaces: aeqi_agents(get) + aeqi_notes(read) + manual prompt assembly.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent": {"type": "string", "description": "Agent name (e.g. 'researcher', 'reviewer', 'architect')"},
                    "task_id": {"type": "string", "description": "Task ID for notes context (e.g. 'sg-010')"},
                    "project": {"type": "string", "description": "Project name"},
                    "prompt": {"type": "string", "description": "Additional instructions for the agent"}
                },
                "required": ["agent", "project"]
            }),
        },
    ];

    // Recall result cache: avoids redundant IPC queries within a session.
    // Key = "project\0query\0scope\0limit", Value = (timestamp, result).
    // Entries older than 5 minutes are treated as stale.
    let mut recall_cache: HashMap<String, (Instant, serde_json::Value)> = HashMap::new();
    const RECALL_CACHE_TTL_SECS: u64 = 300;

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }

        let request: McpRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let response = match request.method.as_str() {
            "initialize" => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.unwrap_or(serde_json::Value::Null),
                result: Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "aeqi", "version": "4.0.0"}
                })),
                error: None,
            },
            "notifications/initialized" => continue,
            "tools/list" => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.unwrap_or(serde_json::Value::Null),
                result: Some(serde_json::json!({"tools": tools})),
                error: None,
            },
            "tools/call" => {
                let tool_name = request
                    .params
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let args = request.params.get("arguments").cloned().unwrap_or_default();

                let result = match tool_name {
                    // ── Discovery ──
                    "aeqi_projects" => {
                        let projects: Vec<serde_json::Value> = config
                            .companies
                            .iter()
                            .map(|p| {
                                serde_json::json!({
                                    "name": p.name,
                                    "prefix": p.prefix,
                                    "repo": p.repo,
                                })
                            })
                            .collect();
                        Ok(serde_json::json!({"ok": true, "projects": projects}))
                    }

                    // ── Primer ──
                    "aeqi_primer" => {
                        let project = args.get("project").and_then(|v| v.as_str()).unwrap_or("");

                        // Config is the source of truth for primers.
                        let project_primer = if project == "shared" {
                            config.shared_primer.clone().unwrap_or_default()
                        } else {
                            config
                                .companies
                                .iter()
                                .find(|p| p.name == project)
                                .and_then(|p| p.primer.clone())
                                .unwrap_or_default()
                        };

                        let shared_primer = if project != "shared" {
                            config.shared_primer.clone().unwrap_or_default()
                        } else {
                            String::new()
                        };

                        let mut parts = Vec::new();
                        if !shared_primer.is_empty() {
                            parts.push(shared_primer);
                        }
                        if !project_primer.is_empty() {
                            parts.push(project_primer);
                        }

                        if parts.is_empty() {
                            Ok(
                                serde_json::json!({"ok": false, "error": format!("no primer found for project '{project}'")}),
                            )
                        } else {
                            let content = parts.join("\n\n---\n\n");
                            Ok(serde_json::json!({
                                "ok": true,
                                "project": project,
                                "content": content,
                            }))
                        }
                    }

                    // ── Skills (knowledge, procedures, checklists) ──
                    "aeqi_skills" => {
                        let action = args
                            .get("action")
                            .and_then(|v| v.as_str())
                            .unwrap_or("list");
                        let project_filter = args.get("project").and_then(|v| v.as_str());
                        let phase_filter = args.get("phase").and_then(|v| v.as_str());
                        let name_filter = args.get("name").and_then(|v| v.as_str());

                        let mut all_skills = Vec::new();
                        all_skills
                            .extend(scan_dir(&base_dir.join("projects/shared/skills"), "shared"));
                        for entry in std::fs::read_dir(base_dir.join("projects"))
                            .into_iter()
                            .flatten()
                            .flatten()
                        {
                            let p = entry.file_name().to_string_lossy().to_string();
                            if p == "shared" {
                                continue;
                            }
                            all_skills.extend(scan_dir(&entry.path().join("skills"), &p));
                        }

                        if action == "get" {
                            let name = name_filter.unwrap_or("");
                            match all_skills.into_iter().find(|s| {
                                s.get("name")
                                    .and_then(|n| n.as_str())
                                    .is_some_and(|n| n == name)
                            }) {
                                Some(s) => Ok(s),
                                None => Ok(
                                    serde_json::json!({"ok": false, "error": format!("skill '{name}' not found")}),
                                ),
                            }
                        } else {
                            let filtered: Vec<serde_json::Value> = all_skills
                                .into_iter()
                                .filter(|s| {
                                    let project_ok = project_filter.is_none_or(|pf| {
                                        let src =
                                            s.get("source").and_then(|v| v.as_str()).unwrap_or("");
                                        src == pf || src == "shared"
                                    });
                                    let phase_ok = phase_filter.is_none_or(|pf| {
                                        s.get("phase")
                                            .and_then(|v| v.as_str())
                                            .is_some_and(|p| p == pf)
                                    });
                                    project_ok && phase_ok
                                })
                                .map(|s| {
                                    serde_json::json!({
                                        "name": s["name"], "source": s["source"],
                                        "phase": s["phase"], "preview": s["preview"],
                                    })
                                })
                                .collect();
                            Ok(
                                serde_json::json!({"ok": true, "count": filtered.len(), "skills": filtered}),
                            )
                        }
                    }

                    // ── Agents (autonomous actor templates) ──
                    "aeqi_agents" => {
                        let action = args
                            .get("action")
                            .and_then(|v| v.as_str())
                            .unwrap_or("list");
                        let project_filter = args.get("project").and_then(|v| v.as_str());
                        let phase_filter = args.get("phase").and_then(|v| v.as_str());
                        let name_filter = args.get("name").and_then(|v| v.as_str());

                        let mut all_agents = Vec::new();
                        all_agents
                            .extend(scan_dir(&base_dir.join("projects/shared/agents"), "shared"));
                        for entry in std::fs::read_dir(base_dir.join("projects"))
                            .into_iter()
                            .flatten()
                            .flatten()
                        {
                            let p = entry.file_name().to_string_lossy().to_string();
                            if p == "shared" {
                                continue;
                            }
                            all_agents.extend(scan_dir(&entry.path().join("agents"), &p));
                        }

                        if action == "get" {
                            let name = name_filter.unwrap_or("");
                            match all_agents.into_iter().find(|a| {
                                a.get("name")
                                    .and_then(|n| n.as_str())
                                    .is_some_and(|n| n == name)
                            }) {
                                Some(a) => Ok(a),
                                None => Ok(
                                    serde_json::json!({"ok": false, "error": format!("agent '{name}' not found")}),
                                ),
                            }
                        } else {
                            let filtered: Vec<serde_json::Value> = all_agents
                                .into_iter()
                                .filter(|a| {
                                    let project_ok = project_filter.is_none_or(|pf| {
                                        let src =
                                            a.get("source").and_then(|v| v.as_str()).unwrap_or("");
                                        src == pf || src == "shared"
                                    });
                                    let phase_ok = phase_filter.is_none_or(|pf| {
                                        a.get("phase")
                                            .and_then(|v| v.as_str())
                                            .is_some_and(|p| p == pf)
                                    });
                                    project_ok && phase_ok
                                })
                                .map(|a| {
                                    serde_json::json!({
                                        "name": a["name"], "source": a["source"],
                                        "phase": a["phase"], "model": a["model"],
                                        "preview": a["preview"],
                                    })
                                })
                                .collect();
                            Ok(
                                serde_json::json!({"ok": true, "count": filtered.len(), "agents": filtered}),
                            )
                        }
                    }

                    // ── Memory ──
                    "aeqi_recall" => {
                        let project = args.get("project").and_then(|v| v.as_str()).unwrap_or("");
                        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                        let scope = args
                            .get("scope")
                            .and_then(|v| v.as_str())
                            .unwrap_or("domain");
                        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5);

                        let cache_key = format!("{project}\0{query}\0{scope}\0{limit}");

                        let cached_hit = recall_cache.get(&cache_key).and_then(|(ts, val)| {
                            if ts.elapsed().as_secs() < RECALL_CACHE_TTL_SECS {
                                Some(val.clone())
                            } else {
                                None
                            }
                        });

                        if let Some(val) = cached_hit {
                            Ok(val)
                        } else {
                            recall_cache.remove(&cache_key);
                            let ipc = serde_json::json!({
                                "cmd": "memories",
                                "project": project,
                                "query": query,
                                "scope": scope,
                                "limit": limit,
                            });
                            let r = ipc_request_sync(&data_dir, &ipc);
                            if let Ok(ref val) = r {
                                recall_cache.insert(cache_key, (Instant::now(), val.clone()));
                            }
                            r
                        }
                    }
                    "aeqi_remember" => {
                        let mut ipc = args.clone();
                        ipc["cmd"] = serde_json::json!("knowledge_store");
                        if ipc
                            .get("scope")
                            .and_then(|v| v.as_str())
                            .is_none_or(|s| s.is_empty())
                        {
                            ipc["scope"] = serde_json::json!("domain");
                        }
                        // Invalidate recall cache for this project — new memories change results.
                        if let Some(project) = args.get("project").and_then(|v| v.as_str()) {
                            let prefix = format!("{project}\0");
                            recall_cache.retain(|k, _| !k.starts_with(&prefix));
                        }
                        ipc_request_sync(&data_dir, &ipc)
                    }

                    // ── Operations ──
                    "aeqi_status" => {
                        let mut ipc = serde_json::json!({"cmd": "status"});
                        if let Some(p) = args.get("project").and_then(|v| v.as_str()) {
                            ipc["project"] = serde_json::json!(p);
                        }
                        ipc_request_sync(&data_dir, &ipc)
                    }
                    "aeqi_notes" => {
                        let action = args
                            .get("action")
                            .and_then(|v| v.as_str())
                            .unwrap_or("read");
                        match action {
                            "post" => {
                                let mut ipc = args.clone();
                                ipc["cmd"] = serde_json::json!("post_notes");
                                ipc["agent"] = serde_json::json!("worker");
                                if ipc.get("durability").and_then(|v| v.as_str()).is_none() {
                                    ipc["durability"] = serde_json::json!("durable");
                                }
                                ipc_request_sync(&data_dir, &ipc)
                            }
                            "get" => {
                                let ipc = serde_json::json!({
                                    "cmd": "get_notes",
                                    "project": args.get("project").and_then(|v| v.as_str()).unwrap_or(""),
                                    "key": args.get("key").and_then(|v| v.as_str()).unwrap_or(""),
                                });
                                ipc_request_sync(&data_dir, &ipc)
                            }
                            "claim" => {
                                let ipc = serde_json::json!({
                                    "cmd": "claim_notes",
                                    "project": args.get("project").and_then(|v| v.as_str()).unwrap_or(""),
                                    "resource": args.get("resource").and_then(|v| v.as_str()).unwrap_or(""),
                                    "content": args.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                                    "agent": "worker",
                                });
                                ipc_request_sync(&data_dir, &ipc)
                            }
                            "release" => {
                                let ipc = serde_json::json!({
                                    "cmd": "release_notes",
                                    "project": args.get("project").and_then(|v| v.as_str()).unwrap_or(""),
                                    "resource": args.get("resource").and_then(|v| v.as_str()).unwrap_or(""),
                                    "agent": "worker",
                                    "force": args.get("force").and_then(|v| v.as_bool()).unwrap_or(false),
                                });
                                ipc_request_sync(&data_dir, &ipc)
                            }
                            "delete" => {
                                let ipc = serde_json::json!({
                                    "cmd": "delete_notes",
                                    "project": args.get("project").and_then(|v| v.as_str()).unwrap_or(""),
                                    "key": args.get("key").and_then(|v| v.as_str()).unwrap_or(""),
                                });
                                ipc_request_sync(&data_dir, &ipc)
                            }
                            _ => {
                                let prefix_filter = args.get("prefix").and_then(|v| v.as_str());
                                let mut ipc = serde_json::json!({
                                    "cmd": "notes",
                                    "project": args.get("project").and_then(|v| v.as_str()).unwrap_or(""),
                                });
                                if let Some(tags) = args.get("tags") {
                                    ipc["tags"] = tags.clone();
                                }
                                if let Some(since) = args.get("since") {
                                    ipc["since"] = since.clone();
                                }
                                if let Some(limit) = args.get("limit") {
                                    ipc["limit"] = limit.clone();
                                }
                                if let Some(cross) = args.get("cross_project") {
                                    ipc["cross_project"] = cross.clone();
                                }
                                let result = ipc_request_sync(&data_dir, &ipc);
                                if let Some(pf) = prefix_filter {
                                    result.map(|mut v| {
                                        if let Some(entries) =
                                            v.get_mut("entries").and_then(|e| e.as_array_mut())
                                        {
                                            entries.retain(|e| {
                                                e.get("key")
                                                    .and_then(|k| k.as_str())
                                                    .is_some_and(|k| k.starts_with(pf))
                                            });
                                        }
                                        v
                                    })
                                } else {
                                    result
                                }
                            }
                        }
                    }
                    "aeqi_delegate" => {
                        let agent_name = args.get("agent").and_then(|v| v.as_str()).unwrap_or("");
                        let project = args.get("project").and_then(|v| v.as_str()).unwrap_or("");
                        let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                        let extra_prompt =
                            args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

                        // 1. Load agent template — try DB via IPC first, fall back to .md files
                        let agent_template = {
                            let mut found = String::new();

                            // Try DB lookup via IPC (agent_info command)
                            if let Ok(resp) = ipc_request_sync(
                                &data_dir,
                                &serde_json::json!({
                                    "cmd": "agent_info",
                                    "name": agent_name,
                                }),
                            ) && resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false)
                                && let Some(sp) = resp.get("system_prompt").and_then(|v| v.as_str())
                                && !sp.is_empty()
                            {
                                found = sp.to_string();
                            }

                            // Fall back to .md files on disk
                            if found.is_empty() {
                                for dir in &[
                                    base_dir.join("projects").join(project).join("agents"),
                                    base_dir.join("projects/shared/agents"),
                                ] {
                                    let path = dir.join(format!("{agent_name}.md"));
                                    if path.exists()
                                        && let Ok(content) = std::fs::read_to_string(&path)
                                    {
                                        found = content;
                                        break;
                                    }
                                }
                            }

                            if found.is_empty() {
                                return Err(anyhow::anyhow!("agent '{agent_name}' not found"));
                            }
                            found
                        };

                        // 2. Gather notes context for the task
                        let mut bb_context = String::new();
                        if !task_id.is_empty() {
                            let bb_req = serde_json::json!({
                                "cmd": "notes",
                                "project": project,
                                "prefix": format!("task:{task_id}"),
                                "limit": 10
                            });
                            if let Ok(bb_resp) = ipc_request_sync(&data_dir, &bb_req)
                                && let Some(entries) =
                                    bb_resp.get("entries").and_then(|e| e.as_array())
                            {
                                for entry in entries {
                                    let key =
                                        entry.get("key").and_then(|k| k.as_str()).unwrap_or("");
                                    let content =
                                        entry.get("content").and_then(|c| c.as_str()).unwrap_or("");
                                    if !content.is_empty() {
                                        bb_context.push_str(&format!("\n## {key}\n{content}\n"));
                                    }
                                }
                            }
                        }

                        // 3. Assemble the delegation prompt
                        let mut prompt = String::new();
                        prompt.push_str(&agent_template);
                        prompt.push_str("\n\n---\n\n");
                        prompt.push_str(&format!("# Delegation Context\n\nProject: {project}\n"));
                        if !task_id.is_empty() {
                            prompt.push_str(&format!("Task: {task_id}\n"));
                        }
                        if !bb_context.is_empty() {
                            prompt.push_str(&format!("\n# Notes Context\n{bb_context}\n"));
                        }
                        if !extra_prompt.is_empty() {
                            prompt.push_str(&format!("\n# Instructions\n{extra_prompt}\n"));
                        }
                        prompt.push_str(&format!(
                            "\nWhen done, post your results to notes:\n\
                             aeqi_notes(action='post', project='{project}', key='task:{task_id}:{agent_name}', content='<your findings>')\n"
                        ));

                        Ok(serde_json::json!({
                            "ok": true,
                            "agent": agent_name,
                            "project": project,
                            "task_id": task_id,
                            "prompt": prompt,
                            "usage": "Pass the 'prompt' field to a Claude Code Agent subagent. The agent will read notes context and post results back."
                        }))
                    }

                    "aeqi_create_task" => {
                        let mut ipc = args.clone();
                        ipc["cmd"] = serde_json::json!("create_task");
                        ipc_request_sync(&data_dir, &ipc)
                    }
                    "aeqi_close_task" => {
                        let project = args
                            .get("project")
                            .and_then(|v| v.as_str())
                            .or_else(|| {
                                args.get("task_id")
                                    .and_then(|v| v.as_str())
                                    .and_then(|id| id.split('-').next())
                            })
                            .unwrap_or("");
                        let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                        let mut ipc = args.clone();
                        ipc["cmd"] = serde_json::json!("close_task");
                        let mut result = ipc_request_sync(&data_dir, &ipc);

                        // Enrich: check if review was posted for this task
                        if let Ok(ref mut val) = result
                            && !task_id.is_empty()
                        {
                            let review_key = format!("task:{task_id}:review");
                            let bb_req = serde_json::json!({
                                "cmd": "notes",
                                "project": project,
                                "prefix": &review_key,
                                "limit": 1
                            });
                            let has_review = ipc_request_sync(&data_dir, &bb_req)
                                .ok()
                                .and_then(|r| r.get("entries")?.as_array().map(|a| !a.is_empty()))
                                .unwrap_or(false);

                            if !has_review {
                                val["review_warning"] = serde_json::json!(format!(
                                    "No review posted for {task_id}. For significant changes, delegate: aeqi_delegate(agent='reviewer', project='{project}', task_id='{task_id}')"
                                ));
                            }
                        }

                        result
                    }

                    "aeqi_graph" => {
                        let project = args.get("project").and_then(|v| v.as_str()).unwrap_or("");
                        let action = args
                            .get("action")
                            .and_then(|v| v.as_str())
                            .unwrap_or("stats");

                        // Find project repo path from config
                        let repo_path =
                            config
                                .companies
                                .iter()
                                .find(|p| p.name == project)
                                .map(|p| {
                                    let r = p.repo.replace(
                                        '~',
                                        &dirs::home_dir().unwrap_or_default().to_string_lossy(),
                                    );
                                    std::path::PathBuf::from(r)
                                });

                        let graph_dir = data_dir.join("codegraph");
                        std::fs::create_dir_all(&graph_dir).ok();
                        let db_path = graph_dir.join(format!("{project}.db"));

                        match action {
                            "index" => {
                                let repo = repo_path.ok_or_else(|| {
                                    anyhow::anyhow!("project '{project}' not found in config")
                                })?;
                                let store = aeqi_graph::GraphStore::open(&db_path)?;
                                let indexer = aeqi_graph::Indexer::new();
                                let result = indexer.index(&repo, &store)?;
                                Ok(serde_json::json!({
                                    "ok": true,
                                    "project": project,
                                    "result": result.to_string(),
                                    "files": result.files_parsed,
                                    "nodes": result.nodes,
                                    "edges": result.edges,
                                    "communities": result.communities,
                                    "processes": result.processes,
                                    "unresolved": result.unresolved,
                                }))
                            }
                            "search" => {
                                let query =
                                    args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10)
                                    as usize;
                                let store = aeqi_graph::GraphStore::open(&db_path)?;
                                let results = store.search_nodes(query, limit)?;
                                Ok(serde_json::json!({
                                    "ok": true,
                                    "count": results.len(),
                                    "nodes": results,
                                }))
                            }
                            "context" => {
                                let node_id =
                                    args.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
                                let store = aeqi_graph::GraphStore::open(&db_path)?;
                                let ctx = store.context(node_id)?;
                                Ok(serde_json::json!({
                                    "ok": true,
                                    "node": ctx.node,
                                    "callers": ctx.callers,
                                    "callees": ctx.callees,
                                    "implementors": ctx.implementors,
                                    "incoming_edges": ctx.incoming_count,
                                    "outgoing_edges": ctx.outgoing_count,
                                }))
                            }
                            "impact" => {
                                let node_id =
                                    args.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
                                let depth =
                                    args.get("depth").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
                                let store = aeqi_graph::GraphStore::open(&db_path)?;
                                let entries = store.impact(&[node_id], depth)?;
                                let affected: Vec<serde_json::Value> = entries
                                    .iter()
                                    .map(|e| {
                                        serde_json::json!({
                                            "node": e.node,
                                            "depth": e.depth,
                                        })
                                    })
                                    .collect();
                                Ok(serde_json::json!({
                                    "ok": true,
                                    "source": node_id,
                                    "affected_count": affected.len(),
                                    "affected": affected,
                                }))
                            }
                            "file" => {
                                let file_path =
                                    args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                                let store = aeqi_graph::GraphStore::open(&db_path)?;
                                let nodes = store.nodes_in_file(file_path)?;
                                Ok(serde_json::json!({
                                    "ok": true,
                                    "file": file_path,
                                    "count": nodes.len(),
                                    "nodes": nodes,
                                }))
                            }
                            "stats" => {
                                let store = aeqi_graph::GraphStore::open(&db_path)?;
                                let stats = store.stats()?;
                                let indexed_at = store.get_meta("indexed_at")?.unwrap_or_default();
                                Ok(serde_json::json!({
                                    "ok": true,
                                    "project": project,
                                    "nodes": stats.node_count,
                                    "edges": stats.edge_count,
                                    "files": stats.file_count,
                                    "indexed_at": indexed_at,
                                }))
                            }
                            "diff_impact" => {
                                let repo = repo_path.ok_or_else(|| {
                                    anyhow::anyhow!("project '{project}' not found")
                                })?;
                                let depth =
                                    args.get("depth").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
                                let store = aeqi_graph::GraphStore::open(&db_path)?;
                                let indexer = aeqi_graph::Indexer::new();
                                let impact = indexer.diff_impact(&repo, &store, depth)?;
                                let changed: Vec<serde_json::Value> = impact.changed_symbols.iter().map(|s| {
                                    serde_json::json!({"name": s.name, "label": s.label, "file": s.file_path, "line": s.start_line})
                                }).collect();
                                let affected: Vec<serde_json::Value> = impact.affected.iter().map(|e| {
                                    serde_json::json!({"name": e.node.name, "label": e.node.label, "file": e.node.file_path, "depth": e.depth})
                                }).collect();
                                Ok(serde_json::json!({
                                    "ok": true,
                                    "changed_files": impact.changed_files,
                                    "changed_symbols": changed,
                                    "affected_count": affected.len(),
                                    "affected": affected,
                                }))
                            }
                            "file_summary" => {
                                let file_path =
                                    args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                                let store = aeqi_graph::GraphStore::open(&db_path)?;
                                let summary = store.file_summary(file_path)?;
                                Ok(serde_json::json!({
                                    "ok": true,
                                    "file": file_path,
                                    "summary": summary,
                                }))
                            }
                            "incremental" => {
                                let repo = repo_path.ok_or_else(|| {
                                    anyhow::anyhow!("project '{project}' not found")
                                })?;
                                let store = aeqi_graph::GraphStore::open(&db_path)?;
                                let indexer = aeqi_graph::Indexer::new();
                                let result = indexer.index_incremental(&repo, &store)?;
                                Ok(serde_json::json!({
                                    "ok": true,
                                    "project": project,
                                    "result": result.to_string(),
                                    "files": result.files_parsed,
                                    "nodes": result.nodes,
                                    "edges": result.edges,
                                }))
                            }
                            "synthesize" => {
                                let community_id = args
                                    .get("community_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let store = aeqi_graph::GraphStore::open(&db_path)?;

                                // Read existing nodes and edges from the graph DB (no re-index)
                                let all_nodes: Vec<aeqi_graph::CodeNode> = {
                                    let mut stmt = store.conn().prepare(
                                        "SELECT id, label, name, file_path, start_line, end_line, language, is_exported, signature, doc_comment, community_id FROM code_nodes"
                                    )?;
                                    stmt.query_map([], |row| {
                                        Ok(aeqi_graph::CodeNode {
                                            id: row.get(0)?,
                                            label: serde_json::from_str(&format!(
                                                "\"{}\"",
                                                row.get::<_, String>(1)?
                                            ))
                                            .unwrap_or(aeqi_graph::NodeLabel::Function),
                                            name: row.get(2)?,
                                            file_path: row.get(3)?,
                                            start_line: row.get(4)?,
                                            end_line: row.get(5)?,
                                            language: row.get(6)?,
                                            is_exported: row.get(7)?,
                                            signature: row.get(8)?,
                                            doc_comment: row.get(9)?,
                                            community_id: row.get(10)?,
                                        })
                                    })?
                                    .filter_map(|r| r.ok())
                                    .collect()
                                };

                                let all_edges: Vec<aeqi_graph::CodeEdge> = {
                                    let mut stmt = store.conn().prepare(
                                        "SELECT source_id, target_id, edge_type, confidence, tier, step FROM code_edges"
                                    )?;
                                    stmt.query_map([], |row| {
                                        Ok(aeqi_graph::CodeEdge {
                                            source_id: row.get(0)?,
                                            target_id: row.get(1)?,
                                            edge_type: serde_json::from_str(&format!(
                                                "\"{}\"",
                                                row.get::<_, String>(2)?
                                            ))
                                            .unwrap_or(aeqi_graph::EdgeType::Uses),
                                            confidence: row.get(3)?,
                                            tier: row.get(4)?,
                                            step: row.get(5)?,
                                        })
                                    })?
                                    .filter_map(|r| r.ok())
                                    .collect()
                                };

                                // Find the community
                                let communities =
                                    aeqi_graph::detect_communities(&all_nodes, &all_edges, 3);
                                let community = if community_id.is_empty() {
                                    communities.first()
                                } else {
                                    communities.iter().find(|c| c.id == community_id)
                                };

                                match community {
                                    Some(comm) => {
                                        let skill = aeqi_graph::synthesize_skill(
                                            comm, &all_nodes, &all_edges,
                                        );
                                        Ok(serde_json::json!({
                                            "ok": true,
                                            "skill_name": skill.name,
                                            "description": skill.description,
                                            "content": skill.content,
                                        }))
                                    }
                                    None => {
                                        Err(anyhow::anyhow!("community '{community_id}' not found"))
                                    }
                                }
                            }
                            _ => Err(anyhow::anyhow!("unknown aeqi_graph action: {action}")),
                        }
                    }

                    _ => Err(anyhow::anyhow!("unknown tool: {tool_name}")),
                };

                match result {
                    Ok(data) => McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id.unwrap_or(serde_json::Value::Null),
                        result: Some(serde_json::json!({
                            "content": [{"type": "text", "text": serde_json::to_string_pretty(&data).unwrap_or_default()}]
                        })),
                        error: None,
                    },
                    Err(e) => McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id.unwrap_or(serde_json::Value::Null),
                        result: Some(serde_json::json!({
                            "content": [{"type": "text", "text": format!("Error: {e}")}],
                            "isError": true
                        })),
                        error: None,
                    },
                }
            }
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.unwrap_or(serde_json::Value::Null),
                result: Some(serde_json::json!({})),
                error: None,
            },
        };

        let resp_json = serde_json::to_string(&response)?;
        writeln!(stdout, "{resp_json}")?;
        stdout.flush()?;
    }

    Ok(())
}
