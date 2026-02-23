# Operating Instructions

Inherits from `rigs/shared/WORKFLOW.md` for code standards, R→D→R pipeline, and escalation.

## Realm-Specific Workflow

Standard worktree workflow applies (same as all domains).

1. Run `cargo test` and `cargo clippy` before committing
2. Commit messages: `feat:`, `fix:`, `docs:`, `chore:`
3. Edition: Rust 2024

## Key Paths

- Binary: `rm/src/main.rs`
- Traits: `crates/realm-core/src/traits/`
- Config: `config/realm.toml`
- Domains: `rigs/<name>/`
- Shared: `rigs/shared/`

## Available Skills

### R→D→R Archetypes (domain-specific overrides)
- **researcher**: Framework analysis — trait hierarchies, orchestration flow, config patterns
- **developer**: Rust implementation — trait design, async/tokio, edition 2024
- **reviewer**: Framework review — trait boundaries, async safety, no hardcoded heuristics

## Adding Things

- New tool: implement `Tool` trait in `realm-tools`, export from lib.rs, add to `build_domain_tools()`
- New provider: implement `Provider` trait in `realm-providers`, export, add factory
- New channel: implement `Channel` trait in `realm-gates`, export, wire into summoner
- New domain: create `rigs/<name>/` with SOUL.md + IDENTITY.md + AGENTS.md, add to `config/realm.toml`

## Critical Rules

- Traits over concrete types — everything through Provider, Tool, Memory, Observer, Channel
- Zero Framework Cognition — agent loop is thin, LLM decides everything
- No hardcoded heuristics in the agent loop
- Shared templates in `rigs/shared/` — never duplicate per-domain what can be shared
