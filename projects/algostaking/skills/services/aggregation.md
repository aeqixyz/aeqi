# Service: aggregation

## Required Reading
1. `.claude/skills/pipelines/data.md`
2. `.claude/skills/crates/types.md` - BarData
3. `.claude/skills/crates/keys.md` - format_bar_topic_binary

## Purpose

Aggregates ticks into AVBB (Adaptive Volatility-Based Bars) and time/tick bars, publishing via ZMQ.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, subscriber setup |
| `src/bar_builder.rs` | Bar construction state machine |
| `src/volatility.rs` | AVBB estimators (Parkinson, Rogers-Satchell, etc.) |
| `src/state.rs` | Per-symbol aggregation state |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `bar_types` | `[PRBB, Time]` | Enabled bar types |
| `bar_types[].threshold` | `0.001` | Volatility threshold |
| `bar_types[].interval_seconds` | `60` | Time bar interval |
| `zmq.sub_endpoint` | `tcp://127.0.0.1:5555` | Tick input |
| `zmq.pub_endpoint` | `tcp://0.0.0.0:5556` | Bar output |

## ZMQ Connections

| Direction | Port | Topic | Description |
|-----------|------|-------|-------------|
| IN | 5555 | Binary 8-byte | Ticks from ingestion |
| OUT | 5556 | Binary 12-byte | Bars (market_key + bar_key) |

## Bar Types

| Type | ID | Trigger | Estimator |
|------|-----|---------|-----------|
| PRBB | 1 | Parkinson range | (H-L)²/(4ln2) |
| PKBB | 2 | Parkinson-Kunitomo | Drift-adjusted |
| RSBB | 3 | Rogers-Satchell | Mean-reversion |
| GKBB | 4 | Garman-Klass | Efficiency-weighted |
| VWBB | 5 | VWAP deviation | Volume-weighted |
| RVBB | 6 | Realized variance | Sum of squared returns |
| Tick | 10 | N trades | Fixed count |
| Time | 11 | T seconds | Fixed interval |

## Testing

```bash
cargo build --release -p aggregation
CONFIG_PATH=config/dev.yaml ./target/release/aggregation
curl http://localhost:9001/metrics | grep bars_emitted
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| No bars | No ticks arriving | Check ingestion |
| Bar gaps | Tick gaps | Check ingestion throughput |
| Wrong volatility | Estimator bug | Check volatility.rs |
