# Sigil

[![CI](https://github.com/0xAEQI/sigil/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/0xAEQI/sigil/actions/workflows/ci.yml)

**Proactive AI.** The agent orchestrator that doesn't wait. It routes to the right agents, executes through Sigil's native agent runtime, verifies its own work, learns from results, and messages you with what got done.

## What It Does

You tell Sigil what you want. It decomposes into tasks, routes to specialist agents, executes in isolated environments, verifies outcomes, extracts knowledge, and wakes you up to a brief. Your notes become reality.

```
Intent → Understand → Orchestrate → Execute → Verify → Learn → Proact
   ↑                                                              │
   └──────────────────────────────────────────────────────────────┘
```

## Architecture

- **9 Rust crates**, 515 tests, zero clippy warnings
- **Composable middleware chain** — 8 implementations: loop detection, guardrails, cost tracking, context compression, context budget, memory refresh, clarification, safety net
- **Verification pipeline** — 5-stage with confidence scoring, three-strikes escalation
- **Memory graph** — relationships, deduplication, hotness scoring (7d half-life), hierarchical L0/L1/L2
- **Intelligent retrieval** — intent-driven query planning, multi-signal scoring (BM25 + vector + hotness + confidence + graph)
- **Notes system** — directives that manifest into tasks with status tracking
- **Proactive engine** — morning briefs, anomaly detection, suggestions, notifications
- **Skill promotion** — recurring patterns auto-promoted to formal skill definitions
- **Web dashboard** — chat-first UI (Vite + React 19), context panel, command palette
- **Chat** — dual-path (quick + agent execution), multi-channel (web, Telegram, API)
- **Monorepo** — Rust workspace plus `apps/ui` frontend with shared release flow

## Quick Start

```bash
cargo build --release
npm --prefix apps/ui install
npm --prefix apps/ui run build
sigil setup --runtime openrouter_agent
sigil daemon start    # orchestration daemon
sigil web start       # web API on :8400, optionally serves apps/ui/dist
```

## Monorepo Layout

```text
sigil/
  crates/    # Rust crates
  sigil-cli/ # CLI binary
  apps/ui/   # Vite + React frontend
  config/    # sample and local config
  docs/      # architecture and notes
```

## Crates

| Crate | Purpose |
|-------|---------|
| `sigil-cli` | CLI binary (28 commands) |
| `sigil-core` | Config, traits, agent loop, identity |
| `sigil-orchestrator` | Daemon, supervisor, worker, chat engine, middleware, verification, notes, proactive engine |
| `sigil-web` | Axum REST API + WebSocket |
| `sigil-tasks` | Task DAG (JSONL), missions |
| `sigil-memory` | SQLite + FTS5 + vector search, memory graph, intelligent retrieval |
| `sigil-providers` | OpenRouter, Anthropic, Ollama |
| `sigil-gates` | Telegram, Discord, Slack |
| `sigil-tools` | Shell, file, git, tasks, delegate, skills |

## UI Runtime

- `apps/ui` is the canonical frontend source.
- `sigil-web` can serve the built SPA directly when `[web].ui_dist_dir` is configured.
- For production, the usual setup is `nginx` or `caddy` as a thin TLS reverse proxy in front of `sigil-web`.
- For local development, run Vite in `apps/ui` on `:5173` and point it at `sigil-web` on `:8400`.

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
