# Service: signal

## Required Reading
1. `.claude/skills/pipelines/strategy.md`
2. `.claude/skills/crates/types.md` - SignalData
3. `.claude/skills/crates/keys.md` - LtcHeadKey

## Purpose

Liquid Time-Constant (LTC) network for multi-resolution signal aggregation and Kelly-based position sizing.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point |
| `src/ltc/network.rs` | LTC network implementation |
| `src/ltc/cell.rs` | LTC cell dynamics |
| `src/aggregator.rs` | Multi-resolution fusion |
| `src/kelly.rs` | Position sizing |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `ltc.time_constant` | `0.1` | LTC time constant |
| `ltc.decay_rate` | `0.99` | Hidden state decay |
| `kelly.risk_multiplier` | `0.25` | Kelly fraction scaler |
| `kelly.max_fraction` | `0.05` | Max position size |
| `zmq.sub_endpoint` | `tcp://127.0.0.1:5558` | Prediction input |
| `zmq.pub_endpoint` | `tcp://0.0.0.0:5561` | Signal output |

## ZMQ Connections

| Direction | Port | Topic | Description |
|-----------|------|-------|-------------|
| IN | 5558 | Binary 14-byte | Predictions |
| OUT | 5561 | Binary 14-byte | Signals |
| OUT | 5563 | String | LTC checkpoints |

## Head Key Strategy

LtcHeadKey is **venue-specific**: `model_key + market_key`
- Different hidden state per venue
- Captures venue-specific market dynamics
- Maintains trajectory continuity

## Signal Fields

| Field | Description |
|-------|-------------|
| `magnitude_pct` | Signed expected move (+ long, - short) |
| `retracement_pct` | Expected pullback for limit entry |
| `cross_resolution_agreement` | [0,1] multi-timeframe agreement |
| `resolution_count` | Contributing resolutions |

## Testing

```bash
cargo build --release -p signal
cargo test --release -p signal
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| Signal flip-flop | Low agreement | Check cross_resolution_agreement |
| Wrong sizing | Stale capital | Check account sync |
