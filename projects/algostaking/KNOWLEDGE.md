# AlgoStaking Project Knowledge

Sub-microsecond alpha extraction engine. 12 Rust microservices, ZeroMQ pub/sub, FlatBuffers serialization, PostgreSQL/TimescaleDB.

## Service Pipeline

```
Exchange WS → Ingestion → Aggregation → Feature → Prediction → Signal → PMS → OMS → EMS
     ↓            ↓            ↓           ↓           ↓          ↓       ↓       ↓
   Raw         Tick       AVBB Bar     Feature    Prediction  Signal   Position  Order
   JSON     FlatBuffer   FlatBuffer    Vector      Tensor     Score    Request  Execution
```

### Data Pipeline
| Service | ZMQ Pub | Metrics | Purpose |
|---------|---------|---------|---------|
| ingestion | 5555 | 9000 | Exchange WS → Tick FlatBuffers (simd-json, <2.5μs) |
| aggregation | 5556 | 9001 | Tick → AVBB multi-volatility bars |
| persistence | — | 9002 | Batch writer to TimescaleDB (COPY, 100-row batches) |

### Strategy Pipeline
| Service | ZMQ Pub | Metrics | Purpose |
|---------|---------|---------|---------|
| feature | 5557 | 9003 | DAG-based feature engineering (<20μs per bar) |
| prediction | 5558 | 9004 | FNO inference, atomic model hot-swap (ArcSwap) |
| signal | 5561 | 9008 | LTC network aggregation, Kelly sizing |

### Trading Pipeline
| Service | ZMQ Pub | Metrics | Purpose |
|---------|---------|---------|---------|
| pms | 5570 | 9012 | Kelly position sizing, risk limits, exposure management |
| oms | 5564 | 9013 | Order state machine, fill tracking, deduplication |
| ems | 5565 | 9009 | Venue connect, paper/live routing, fill reporting |

### Gateway
| Service | Port | Metrics | Purpose |
|---------|------|---------|---------|
| configuration | — | 9007 | Config distribution |
| api | 8082 | 9010 | REST API (Axum + sqlx) |
| stream | 8081 | 9011 | WebSocket (live data) |

## ZMQ Binary Topics

Hot-path topics use binary format (native endian):

| Port | Topic Format | Size | Payload |
|------|-------------|------|---------|
| 5555 | `[market_key:i64]` | 8 bytes | Tick FlatBuffer |
| 5556 | `[market_key:i64][bar_key:i32]` | 12 bytes | Bar FlatBuffer |
| 5557 | `[market_key:i64][bar_key:i32][feature_id:u16]` | 14 bytes | FeatureVector |
| 5558 | Same as 5557 | 14 bytes | Prediction |
| 5561 | Same as 5557 | 14 bytes | Signal |
| 5564-5572 | String topics | — | Trading pipeline |

## AVBB Bar Types

| Type | ID | Trigger | Use Case |
|------|-----|---------|----------|
| PRBB | 1 | Parkinson range | Volatility clustering |
| PKBB | 2 | Parkinson-Kunitomo | Drift-adjusted |
| RSBB | 3 | Rogers-Satchell | Mean reversion |
| GKBB | 4 | Garman-Klass | Efficiency-weighted |
| VWBB | 5 | VWAP deviation | Volume-weighted |
| RVBB | 6 | Realized variance | Statistical |
| Tick | 10 | N trades | Fixed count |
| Time | 11 | T seconds | Fixed time |

## Prediction Architecture

- **FNO**: Fourier Neural Operator — venue-agnostic, shared spectral encoders
- **LTC**: Liquid Time-Constant networks — venue-specific hidden states
- Additive delta pattern: FNO base + `tanh × scale` LTC refinement
- Untrained LTC → delta ≈ 0 → FNO passthrough
- Retracement: signed ratio of magnitude (opposite sign), temperature-scaled horizon
- Atomic model hot-swap via ArcSwap (zero-allocation)
- Workers are OS threads (crossbeam channels), not tokio

**Head key types:**
- `FnoHeadKey`: venue-agnostic (share across venues)
- `LtcHeadKey`: venue-specific (separate hidden state per venue)

## Trading Architecture

**OpenTrade**: 128 bytes fixed binary (PMS → OMS)

**Order state machine:**
```
PendingNew → New → Acknowledged → PartiallyFilled → Filled
                                                    ↗
                 → PendingCancel → Cancelled
                 → Rejected
```

**Kelly sizing**: `(edge / variance) × quality × risk_multiplier`

**Risk checks**: drawdown limits, daily drawdown, correlation filter

**Fill dedup**: LRU cache (10K entries)

**Paper/Live routing**: per-account flag, EMS handles both

## Benchmark System

- SYSTEM_BENCHMARK fund with dynamic subscriptions
- BenchmarkAllocator capacity: DYNAMIC = strategies_discovered × budget_per_strategy ($1K each)
- 479 subscriptions auto-created across 9 venues
- Each new strategy: add_capital() into account AND fund StatsTracker

## Database

| Env | Database | User | Prometheus |
|-----|----------|------|-----------|
| DEV | algostaking_dev | algo_dev | :9090 |
| PROD | algostaking_prod | algo_prod | :9091 |

- PostgreSQL 16 + TimescaleDB 2.25.0 (Apache 2)
- Schema files: `infrastructure/schema/*.sql` (SOURCE OF TRUTH)
- Config: `/etc/postgresql/16/main/conf.d/01-algostaking-tuning.conf`
- shared_buffers: 8GB, effective_cache_size: 96GB, NVMe-optimized
- Retention: `/etc/algostaking/retention-policy.sh` daily at 4am
- API uses sqlx (Axum), PMS uses deadpool_postgres (tokio-postgres)
- tokio-postgres CANNOT serialize f64/i64 to DECIMAL — compute in SQL

## Environments

| Component | DEV | PROD |
|-----------|-----|------|
| API | dev.api.algostaking.com | api.algostaking.com |
| App | dev.app.algostaking.com | app.algostaking.com |
| Grafana | dev.grafana.algostaking.com | grafana.algostaking.com |
| Services | algostaking-dev-* | algostaking-prod-* |

## Management

```bash
algostaking start|stop|restart|status|health dev|prod [service]
algostaking logs dev api
```

## Infrastructure

- Server: Hetzner, 128GB RAM, 2x NVMe 3.8TB RAID, Ubuntu 24.04
- IP: 5.9.83.245, SSH: 49221
- Monitoring: Prometheus + Grafana + AlertManager (19 rules, 6 groups)
- Email: Postfix send-only SMTP (needs SPF record)
- Secrets: `/etc/algostaking/secrets/` (symlinked as .env)
- Backups: daily 3:00 AM → `/var/backups/algostaking/`

## Shared Crates

| Crate | Purpose |
|-------|---------|
| types | FlatBuffer schemas, data structs (TickData, BarData, SignalData, OpenTrade) |
| keys | Key packing (MarketKey, BarKey, StrategyKey), binary ZMQ topic formatting |
| ports | Port registry (constants for all 12 services) |
| zmq_transport | ResilientSubscriber/Publisher, MessageParser trait, auto-reconnect |
| metrics | HFT metrics (Prometheus) |
| service | Config loading, graceful shutdown |

## Staking Business Logic

- DB: stake_amount, profit_cap, realized_profit, is_capped, free_stake_balance, is_active
- API: stake, staking-summary, account/balance, pause/start
- PMS gate checks + realized_profit tracking
- Frontend: progress bar, stake action, pause/start toggle, profile stats

## Hard-Won Rules

1. **NEVER use `recv()` in `tokio::select!`** — always `recv_timeout()`. Resets heartbeat on cancel.
2. **NEVER do slow async work inside `tokio::select!` arms** — defer with flag. Future WILL be cancelled.
3. **NEVER block inside `tokio::spawn`** — use `try_recv` + `tokio::time::sleep`. Starves runtime.
4. **Read before free in slot-based structures** — extract data BEFORE close/free.
5. **ON CONFLICT requires unique index** — verify with `\d tablename`. Missing = silent data corruption.
6. **tokio-postgres can't serialize f64/i64 to DECIMAL** — compute in SQL subqueries.
7. **account_id = subscription_id** in trading tables. JOIN path: strategy_subscriptions → subaccounts → fund_id.
8. **Schema = source of truth** — fresh `psql -f *.sql` must create working DB.

## Trading Performance (2026-02-13)

- Pre-fee positive (+$4.13K) but fees ($16.26K) eat the edge
- Entry fill rate: 37%, SL loss rate: 99.4%, real leverage: 0.26x
- EMS leverage=3.0 is dead code — PMS allocator handles all sizing
- All horizon exits with trailing stops → classified as TrailingStop (exit_reason=4)

## Pending Work (2026-02-13)

- PMS starting_equity fix: saved but NOT deployed (needs build, commit, purge, restart)
- FNO encoder training: stalled at 0 steps/sec (LTC head works at 30 steps/sec)
- OMS→PMS refactor plan exists but not started
