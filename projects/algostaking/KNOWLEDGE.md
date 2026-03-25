# AlgoStaking Project Knowledge

Lunar-epoch market making system. 15 Rust microservices, ZeroMQ pub/sub, FlatBuffers serialization, PostgreSQL/TimescaleDB.

## Service Pipeline

```
Exchange WS → Ingestion → Aggregation → Feature → Prediction → Signal
  → PMS → RMS → MMS → OMS → EMS
```

Alpha expressed as quote skew (market making), not directional trades. Earns spread + rebates.

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
| optimizer | (batch) | 9016 | Walk-forward PFE brute-force at lunar epoch boundaries (~7.4 days) |

### Trading Pipeline
| Service | ZMQ Pub | Metrics | Purpose |
|---------|---------|---------|---------|
| pms | 5570, 5577 | 9012 | Kelly position sizing, allocation directives |
| rms | 5578-5579 | 9014 | **NEW (2026-03-23)** Independent risk gate: circuit breaker, position/venue/sector/portfolio limits, configurable throttle delay |
| mms | 5580 | 9015 | **NEW (2026-03-23)** Avellaneda-Stoikov quote engine: L2 microprice, funding-adjusted reservation pricing, 5-level geometric ladder |
| oms | 5564 | 9013 | Quote-native order management: cancel-before-replace, 5 bid + 5 ask per market |
| ems | 5565-5566, 5568 | 9009 | Venue connect, paper/live routing, fill reporting |

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
| 5564-5580 | String topics | — | Trading pipeline (PMS/RMS/MMS/OMS/EMS) |

## Trading Data Streams (RMS/MMS)

| Port | Data | Publisher | Consumers |
|------|------|-----------|-----------|
| 5577 | AllocationDirectives | PMS | RMS, Persistence |
| 5578 | ApprovedDirectives | RMS | MMS, Persistence |
| 5579 | RiskCommands | RMS | MMS, OMS, Persistence |
| 5580 | QuoteIntents | MMS | OMS, Persistence |
| 5581 | BookDiffs | Ingestion | Aggregation, Persistence |
| 5582 | L2Snapshots | Aggregation | MMS, Feature, Persistence |
| 5583 | FundingRates | Ingestion | MMS, Persistence |

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

**Risk checks (PMS)**: drawdown limits, daily drawdown, correlation filter

**RMS risk gate**: independent position reconstruction from fills; circuit breaker with graduated response; position/venue/sector/portfolio limits; configurable throttle delay

**MMS quote engine**: Avellaneda-Stoikov reservation pricing; L2 microprice for fair value; funding-rate adjustment; 5-level geometric bid/ask ladder

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
| types | FlatBuffer schemas, message types (TickData, BarData, AllocationDirective, QuoteIntent, etc.) |
| keys | MarketKey packing, topic routing |
| ports | Compile-time port registry (30 ZMQ + 12 metrics) |
| zmq_transport | Resilient PUB/SUB with reconnection |
| metrics | Lock-free Prometheus metrics server |
| service | Config loading, graceful shutdown |
| state_sync | DB bootstrap + ZMQ live cache |
| hindsight | Optimal hindsight tracking for training labels |
| lunar | Moon phase calculator for epoch system |

## Staking Business Logic

- DB: stake_amount, profit_cap, realized_profit, is_capped, free_stake_balance, is_active
- API: stake, staking-summary, account/balance, pause/start
- PMS gate checks + realized_profit tracking
- Frontend: progress bar, stake action, pause/start toggle, profile stats

## Recent Changes (2026-03-25)

- **Config validation**: MMS and RMS `Config::validate()` rejects invalid params at startup (no silent bad config)
- **Fair value validation**: `QuoteEngine.generate_quotes()` returns `Option<QuoteIntent>` — `None` on invalid fair_value; metric: `mms_quotes_skipped_invalid_fv`
- **Should-requote guard**: prevents NaN propagation from zero fair values
- **Optimizer scoring**: unknown methods now warn instead of silently falling back
- **Config externalization**: hardcoded TTL, level-spacing, throttle-delay moved to config files
- **Per-reason rejection metrics**: `rms_rejected_by_reason{reason=...}`, `rms_directives_throttled`

## Hard-Won Rules

1. **NEVER use `recv()` in `tokio::select!`** — always `recv_timeout()`. Resets heartbeat on cancel.
2. **NEVER do slow async work inside `tokio::select!` arms** — defer with flag. Future WILL be cancelled.
3. **NEVER block inside `tokio::spawn`** — use `try_recv` + `tokio::time::sleep`. Starves runtime.
4. **Read before free in slot-based structures** — extract data BEFORE close/free.
5. **ON CONFLICT requires unique index** — verify with `\d tablename`. Missing = silent data corruption.
6. **tokio-postgres can't serialize f64/i64 to DECIMAL** — compute in SQL subqueries.
7. **account_id = subscription_id** in trading tables. JOIN path: strategy_subscriptions → subaccounts → fund_id.
8. **Schema = source of truth** — fresh `psql -f *.sql` must create working DB.
9. **QuoteEngine returns Option** — callers MUST handle None (invalid fair value). Never assume quotes are always produced.
10. **Config structs MUST have `validate()`** called after deserialization — never trust raw TOML/JSON.
11. **Monitoring metrics MUST exist for every rejection/skip/failure path** — silent drops are invisible bugs.

## Trading Performance (2026-02-13)

- Pre-fee positive (+$4.13K) but fees ($16.26K) eat the edge
- Entry fill rate: 37%, SL loss rate: 99.4%, real leverage: 0.26x
- EMS leverage=3.0 is dead code — PMS allocator handles all sizing
- All horizon exits with trailing stops → classified as TrailingStop (exit_reason=4)

## Pending Work (2026-02-13)

- PMS starting_equity fix: saved but NOT deployed (needs build, commit, purge, restart)
- FNO encoder training: stalled at 0 steps/sec (LTC head works at 30 steps/sec)
- OMS→PMS refactor plan exists but not started
