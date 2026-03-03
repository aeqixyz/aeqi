# Operating Instructions

Inherits from `projects/shared/WORKFLOW.md` for code standards, R→D→R pipeline, and escalation.

## Build & Deploy

- Build: `cargo build --release` (7MB binary, LTO + strip)
- Test: `cargo test`
- Lint: `cargo clippy`
- Deploy: merge to `master` → post-merge hook builds + deploys
- Service: `gacha-agency.service` on port 3100

## System-Specific Workflow

Standard worktree workflow applies (same as all projects).

1. Run `cargo test` and `cargo clippy` before committing
2. Commit messages: `feat:`, `fix:`, `docs:`, `chore:`
3. Edition: Rust 2024

## Key Paths

- Binary: `rm/src/main.rs`
- Traits: `crates/system-core/src/traits/`
- Config: `config/system.toml`
- Projects: `projects/<name>/`
- Shared: `projects/shared/`

## Available Skills

### R→D→R Archetypes (project-specific overrides)
- **researcher**: Framework analysis — trait hierarchies, orchestration flow, config patterns
- **developer**: Rust implementation — trait design, async/tokio, edition 2024
- **reviewer**: Framework review — trait boundaries, async safety, no hardcoded heuristics

## Adding Things

- New tool: implement `Tool` trait in `system-tools`, export from lib.rs, add to `build_project_tools()`
- New provider: implement `Provider` trait in `system-providers`, export, add factory
- New channel: implement `Channel` trait in `system-gates`, export, wire into daemon
- New project: create `projects/<name>/` with PERSONA.md + IDENTITY.md + AGENTS.md, add to `config/system.toml`

## Critical Rules

- Traits over concrete types — everything through Provider, Tool, Memory, Observer, Channel
- Zero Framework Cognition — agent loop is thin, LLM decides everything
- No hardcoded heuristics in the agent loop
- Shared templates in `projects/shared/` — never duplicate per-project what can be shared
