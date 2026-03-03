# Service: feature

## Required Reading
1. `.claude/skills/pipelines/strategy.md`
2. `.claude/skills/crates/types.md` - FeatureData
3. `.claude/skills/feature-engineering.md`

## Purpose

DAG-based feature engineering on bars, computing technical indicators with lookback windows and normalization.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point |
| `src/dag.rs` | Feature DAG computation |
| `src/features/` | Individual feature implementations |
| `src/lookback.rs` | Ring buffer lookback windows |
| `src/normalizer.rs` | Z-score, rank, percentile |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `warmup.min_bars` | `100` | Bars before feature ready |
| `lookbacks` | `[60s, 5m, 15m, 1h]` | Lookback periods |
| `zmq.sub_endpoint` | `tcp://127.0.0.1:5556` | Bar input |
| `zmq.pub_endpoint` | `tcp://0.0.0.0:5557` | Feature output |

## ZMQ Connections

| Direction | Port | Topic | Description |
|-----------|------|-------|-------------|
| IN | 5556 | Binary 12-byte | Bars |
| OUT | 5557 | Binary 14-byte | Features (+ feature_id) |

## Feature Categories

| Category | Examples | Lookbacks |
|----------|----------|-----------|
| Momentum | ROC, RSI, MACD | 5m, 15m, 1h |
| Mean Reversion | Bollinger %B, Z-score | 1h, 4h, 1d |
| Microstructure | Spread, Imbalance | 1m, 5m |
| Volume | OBV, Volume Ratio | 15m, 1h |
| Volatility | ATR, Parkinson | 1h, 4h, 1d |

## Testing

```bash
cargo build --release -p feature
cargo test --release -p feature
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| NaN features | Insufficient warmup | Check warmup_bars_remaining |
| Slow computation | SIMD not enabled | Check compiler flags |
