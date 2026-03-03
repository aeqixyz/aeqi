# Service: ems (Execution Management System)

## Required Reading
1. `.claude/skills/pipelines/trading.md`
2. `.claude/skills/crates/types.md` - OrderEvent, FillEvent

## Purpose

Multi-account order execution with paper/live routing. **CRITICAL CODE**.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point |
| `src/executor.rs` | Paper vs live routing |
| `src/paper/simulator.rs` | Paper trading simulation |
| `src/live/binance.rs` | Binance connector |
| `src/live/bybit.rs` | Bybit connector |
| `src/account_state.rs` | State broadcasting |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `paper.slippage_bps` | `5` | Simulated slippage |
| `paper.fill_probability` | `0.95` | Limit fill probability |
| `zmq.sub_orders` | `tcp://127.0.0.1:5564` | Orders from OMS |
| `zmq.pub_fills` | `tcp://0.0.0.0:5565` | Fills to OMS |
| `zmq.pub_account_state` | `tcp://0.0.0.0:5566` | Account state |

## ZMQ Connections

| Direction | Port | Topic | Description |
|-----------|------|-------|-------------|
| IN | 5564 | String | Orders from OMS |
| OUT | 5565 | String | Fills to OMS |
| OUT | 5566 | String | Account state (1s periodic) |
| OUT | 5568 | String | Order acks to OMS |

## Paper vs Live

Account's `is_live` flag (from fund.is_live) is the **SINGLE SOURCE OF TRUTH**:
- `is_live = false` → Paper execution (simulated fills)
- `is_live = true` → Live execution (real exchange)

## Testing

```bash
cargo build --release -p ems
EMS_PAPER_MODE=true cargo test -p ems --test integration
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| No fills | Connector disconnected | Check exchange status |
| Wrong mode | is_live flag wrong | Check fund configuration |
