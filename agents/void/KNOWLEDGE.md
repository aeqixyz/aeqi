# Operational Knowledge

## Sigil/System Architecture

- 8 Rust crates, ~7MB release binary, 20 CLI commands
- Executor spawns `claude -p` with `--permission-mode bypassPermissions`
- CLAUDECODE env var must be unset to avoid nested-session block
- DispatchBus is in-memory only (no durability across restarts)
- Task IDs are hierarchical: prefix-NNN format

## Known Footguns

### tokio::select! Rules
- NEVER use `recv()` — always `recv_timeout()` (heartbeat reset on cancel)
- NEVER do slow async work inside arms — defer with flag (future WILL be cancelled)
- NEVER block inside `tokio::spawn` — use `try_recv` + async sleep (starves runtime)

### Database Rules
- ON CONFLICT requires unique index — missing = silent data corruption
- tokio-postgres can't serialize f64/i64 to DECIMAL — compute in SQL
- Schema files = source of truth — fresh psql must create working DB

### Slot-Based Structures
- Read before free — extract data BEFORE close/free (application-level use-after-free)

## Infrastructure

- Server: 5.9.83.245, SSH port 49221
- PostgreSQL 16 + TimescaleDB 2.25.0
- Prometheus + Grafana + AlertManager
- 128GB RAM, 2x NVMe 3.8TB RAID, Ubuntu 24.04
