# Operating Instructions

Inherits from `projects/shared/WORKFLOW.md` for git workflow, code standards, R→D→R pipeline, and escalation.

## AlgoStaking-Specific Workflow

1. Work in worktrees: `git worktree add ~/worktrees/feat/<name> -b feat/<name>`
2. Build: `cargo build --release` (must pass before commit)
3. Merge to `dev` → auto-deploys all 12 services
4. Test on dev.api.algostaking.com / dev.app.algostaking.com
5. Merge `dev` → `master` for production (requires Emperor approval)
6. Cleanup worktree after merge

## Code Standards (NON-NEGOTIABLE)

Extends shared standards:
- Zero-allocation hot paths: no Vec::new(), String::new(), Box::new() in per-tick code
- No clone() without justification, no Arc::new() per message
- No HashMap in hot path (prefer DashMap or arrays)
- No Mutex in hot path, no locks held during I/O or await
- No unwrap() in PMS/OMS/EMS (AUTOMATIC FAIL)
- All state transitions must be validated (can_transition_to)
- Fill deduplication must be present
- Paper/live routing must be correct

## Build & Deploy

- Build: `cargo build --release`
- Lint: `cargo clippy --workspace`
- Test: `cargo test`
- Deploy: merge to `dev` → post-merge hook builds + deploys all 12 services
- Manual deploy: copy binary to `/usr/local/bin/`, restart systemd services
- Service management: `algostaking start|stop|restart|status|health dev|prod [service]`
- Systemd: `/etc/systemd/system/algostaking-*`

## Available Skills

Skills are located at the project skills directory. Read the relevant skill file
before starting work on that area:
- `skills/pipelines/` — Data, strategy, trading, gateway pipeline semantics
- `skills/services/` — Per-service implementation guides (12 services)
- `skills/crates/` — Shared crate APIs (types, zmq_transport, keys, ports, metrics)
- `skills/infrastructure/` — Database, systemd, nginx, monitoring, secrets

### R→D→R Archetypes (project-specific overrides)
- **researcher**: HFT codebase exploration — pipeline mapping, ZMQ topology, FlatBuffer schemas
- **developer**: Rust implementation — zero-alloc hot paths, worktree workflow, cargo build
- **code-reviewer**: HFT anti-pattern detection — allocations, locks, select! safety, state machines

### Operational Skills
- **troubleshooter**: Diagnose service failures (systemctl, journalctl, ports, ZMQ)
- **health-checker**: Quick scan of all 12 services, databases, monitoring
- **deploy-watcher**: Verify deployments after merge (binary timestamps, service health)
- **latency-debugger**: Profile HFT pipeline (P50/P99/P999, EMS/PMS/OMS timing)
- **log-analyzer**: Parse service logs for patterns and anomalies
- **metrics-query**: Query Prometheus (PromQL against :9090 dev, :9091 prod)
- **db-inspector**: PostgreSQL/TimescaleDB health, schema, chunks, slow queries

### Subagents
Specialized subagent configs for spawning sub-workers are in `subagents/`.

## Critical Rules

- Never edit files in `/var/www/` (auto-deployed)
- Never commit secrets — use `/etc/algostaking/secrets/`
- NEVER use `recv()` in `tokio::select!` — always `recv_timeout()`
- NEVER do slow async work inside `tokio::select!` arms — defer with flag
- NEVER block inside `tokio::spawn` — use `try_recv` + async sleep
- ON CONFLICT requires unique index — verify with `\d tablename`
- Schema files (`infrastructure/schema/*.sql`) = source of truth — always update
