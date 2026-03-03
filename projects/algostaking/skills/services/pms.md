# Service: pms (Portfolio Management System)

## Required Reading
1. `.claude/skills/pipelines/trading.md`
2. `.claude/skills/crates/types.md` - OpenTrade, SignalData

## Purpose

Signal → OpenTrade conversion with Kelly sizing, risk limits, and account selection. **CRITICAL CODE**.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point |
| `src/sizing.rs` | Kelly criterion sizing |
| `src/risk.rs` | Risk limit checks |
| `src/accounts.rs` | Account selection |
| `src/intent.rs` | OpenTrade construction |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `risk.max_kelly_fraction` | `0.05` | Max position fraction |
| `risk.max_position_pct` | `0.10` | Max position % of capital |
| `risk.max_drawdown_pct` | `0.10` | Max total drawdown |
| `risk.max_daily_drawdown_pct` | `0.05` | Max daily drawdown |
| `zmq.sub_endpoint` | `tcp://127.0.0.1:5561` | Signal input |
| `zmq.pub_endpoint` | `tcp://0.0.0.0:5570` | OpenTrade output |

## ZMQ Connections

| Direction | Port | Topic | Description |
|-----------|------|-------|-------------|
| IN | 5561 | Binary 14-byte | Signals |
| IN | 5566 | String | Account state from EMS |
| IN | 5571 | String | Position updates from OMS |
| OUT | 5570 | Binary 8-byte market_key | OpenTrade (128 bytes) |

## Risk Checks

All positions must pass:
1. **Drawdown limit** - Total and daily
2. **Position limit** - Max % of capital per position
3. **Exposure limit** - Total exposure across positions
4. **Correlation check** - No stacking correlated positions

## Testing

```bash
cargo build --release -p pms
cargo test --release -p pms
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| No trades | All risk rejected | Check risk.rejections metrics |
| Oversized | Wrong capital | Check account state sync |
