# AlgoStaking Infrastructure Overview

## Server
- **IP**: 5.9.83.245 (IPv6: 2a01:4f8:162:310::2)
- **OS**: Ubuntu 24.04
- **Provider**: Hetzner

## Architecture

```
                    ┌─────────────────────────────────────────┐
                    │              NGINX (443/80)              │
                    │  SSL termination, reverse proxy          │
                    └─────────────────┬───────────────────────┘
                                      │
        ┌─────────────────────────────┼─────────────────────────────┐
        │                             │                             │
        ▼                             ▼                             ▼
   ┌─────────┐                  ┌─────────┐                  ┌─────────┐
   │ API     │                  │ Stream  │                  │ Grafana │
   │ :8082   │                  │ :8081   │                  │ :3000   │
   └────┬────┘                  └────┬────┘                  └────┬────┘
        │                             │                             │
        │         ┌───────────────────┴───────────────────┐         │
        │         │           ZeroMQ Mesh                 │         │
        │         │  (pub/sub between all services)       │         │
        │         └───────────────────────────────────────┘         │
        │                             │                             │
        ▼                             ▼                             ▼
   ┌─────────────────────────────────────────────────────────────────┐
   │                     12 Rust Microservices                       │
   │  ingestion → aggregation → persistence                          │
   │  feature → prediction → signal                                  │
   │  pms → oms → ems                                                │
   │  configuration                                                  │
   └─────────────────────────────────────────────────────────────────┘
        │
        ▼
   ┌─────────────────────────────────────────────────────────────────┐
   │                    TimescaleDB (PostgreSQL)                     │
   │  algostaking_dev / algostaking_prod                             │
   └─────────────────────────────────────────────────────────────────┘
```

## Environments

| Component | DEV | PROD |
|-----------|-----|------|
| Services | algostaking-dev-* | algostaking-prod-* |
| Database | algostaking_dev | algostaking_prod |
| API | dev.api.algostaking.com | api.algostaking.com |
| App | dev.app.algostaking.com | app.algostaking.com |
| Grafana | dev.grafana.algostaking.com | grafana.algostaking.com |
| Prometheus | localhost:9090 | localhost:9091 |

## Key Directories

| Path | Purpose |
|------|---------|
| `/home/claudedev/algostaking-backend` | Backend repo |
| `/var/www/algostaking/{dev,prod}` | Deployed apps |
| `/etc/algostaking/` | Environment files |
| `/etc/systemd/system/algostaking-*` | Service units |
| `/etc/prometheus/` | Prometheus configs |
| `/etc/grafana/` | Grafana DEV config |
| `/etc/grafana-prod/` | Grafana PROD config |
| `/etc/nginx/sites-available/` | Nginx configs |

## Management Commands

```bash
# Service management
algostaking start|stop|restart|status|health dev|prod
algostaking logs dev api

# View all service status
systemctl list-units 'algostaking-dev-*'

# Monitoring
curl http://localhost:9090/targets  # Prometheus targets
curl http://localhost:3000/api/health  # Grafana health
```

## CI/CD Flow

```
git push to dev branch
        │
        ▼
post-commit hook triggers
        │
        ▼
cargo build --release
        │
        ▼
algostaking restart dev
        │
        ▼
Services running with new code
```

## Port Map

### DEV Services
| Port | Service |
|------|---------|
| 8081 | Stream (WebSocket + metrics) |
| 8082 | API (REST + metrics) |
| 9000 | Ingestion metrics |
| 9001 | Aggregation metrics |
| 9002 | Persistence metrics |
| 9003 | Feature metrics |
| 9004 | Prediction metrics |
| 9007 | Configuration metrics |
| 9008 | Signal metrics |
| 9009 | EMS metrics |
| 9012 | PMS metrics |
| 9013 | OMS metrics |

### ZMQ Ports (internal)
| Port | Purpose | Topic Size |
|------|---------|------------|
| 5555 | Tick pub | 8 bytes `[market_key:i64]` |
| 5556 | Bar pub | 12 bytes `[market_key:i64][bar_key:i32]` |
| 5557 | Feature pub | 14 bytes `[market_key:i64][bar_key:i32][feature_id:u16]` |
| 5558 | Prediction pub | 14 bytes (same format) |
| 5561 | Signal pub | 14 bytes (same format) |
| 5564-5572 | Trading pipeline | String topics |

**Note:** Hot-path topics use binary format (native endian). See `keys-packing.md` for details.

## Related Skills

| Skill | Use for |
|-------|---------|
| `keys-packing.md` | Key types (MarketKey, BarKey, StrategyKey), binary topics |
| `zmq.md` | ZMQ port details, topic functions |
| `pipeline-semantics.md` | Full data flow, stage details |
| `websocket-subscriptions.md` | WebSocket channel subscriptions, snapshots |
