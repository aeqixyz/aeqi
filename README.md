# Sigil

[![CI](https://github.com/0xAEQI/sigil/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/0xAEQI/sigil/actions/workflows/ci.yml)

**The AI Company Orchestrator.** Run your entire company through teams of AI agents. One dashboard, every project, full control.

## What It Does

You tell Sigil what you want. It routes to the right agent, executes via Claude Code, learns from the result, and builds your knowledge base automatically. You wake up to a daily brief showing what got done overnight.

```
User → Rei (system leader) → Project Teams → Claude Code Workers
         ↓
    Web Dashboard (entity.business)
    Telegram / WhatsApp channels
```

## Architecture

- **9 Rust crates**, 222+ tests, zero clippy warnings
- **28 CLI commands** including `sigil daemon start` and `sigil web start`
- **Web dashboard** — Vite + React 19 with VS Code-style tree navigation
- **Knowledge system** — SQLite + FTS5 + vector embeddings with hybrid search
- **Chat** — knowledge-aware with memory search, note storage, auto-insight extraction
- **Execution** — one adaptive pipeline with stable agent ownership, strong verification, and fallback-safe orchestration parsing

## Quick Start

```bash
cargo build --release
sigil setup --runtime openrouter_claude_code
sigil daemon start    # orchestration daemon
sigil web start       # web API on :8400
```

## Crates

| Crate | Purpose |
|-------|---------|
| `sigil-cli` | CLI binary (28 commands) |
| `sigil-core` | Config, traits, agent loop, identity |
| `sigil-orchestrator` | Daemon, supervisor, worker, chat engine, memory, audit, expertise |
| `sigil-web` | Axum REST API + WebSocket |
| `sigil-tasks` | Task DAG (JSONL), missions |
| `sigil-memory` | SQLite + FTS5 + vector search |
| `sigil-providers` | OpenRouter, Anthropic, Ollama |
| `sigil-gates` | Telegram, Discord, Slack |
| `sigil-tools` | Shell, file, git, tasks, delegate, skills |

## Key Concepts

- **Projects** — products you're building (repos, tasks, teams, budgets)
- **Agents** — AI personalities with expertise (engineer, trader, designer, researcher, reviewer)
- **Rei** — the system leader, routes all work, multi-archetype personality
- **Skills** — reusable capability templates (developer, health-checker, latency-debugger)
- **Adaptive Pipeline** — one disciplined Discover → Plan → Implement → Verify → Finalize execution flow, with depth adjusted to task scope rather than named pipeline classes
- **Missions** — groups of tasks with progress tracking
- **Memory** — per-project learned knowledge that compounds over time
- **Blackboard** — ephemeral shared knowledge between agents
- **Watchdogs** — event-driven alert rules
- **Cron** — scheduled automation

## License

MIT
