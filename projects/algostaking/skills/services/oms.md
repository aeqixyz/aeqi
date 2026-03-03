# Service: oms (Order Management System)

## Required Reading
1. `.claude/skills/pipelines/trading.md`
2. `.claude/skills/crates/types.md` - ManagedOrder, ManagedOrderStatus

## Purpose

Order lifecycle management with state machine, fill tracking, and deduplication. **CRITICAL CODE**.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point |
| `src/order.rs` | ManagedOrder state machine |
| `src/fills.rs` | Fill processing, deduplication |
| `src/position.rs` | Position tracking |
| `src/audit.rs` | Audit event emission |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `dedup.lru_size` | `10000` | Fill deduplication cache |
| `zmq.sub_targets` | `tcp://127.0.0.1:5570` | OpenTrade input |
| `zmq.sub_fills` | `tcp://127.0.0.1:5565` | Fills from EMS |
| `zmq.sub_acks` | `tcp://127.0.0.1:5568` | Acks from EMS |
| `zmq.pub_orders` | `tcp://0.0.0.0:5564` | Orders to EMS |

## ZMQ Connections

| Direction | Port | Topic | Description |
|-----------|------|-------|-------------|
| IN | 5570 | Binary 8-byte | OpenTrade from PMS |
| IN | 5565 | String | Fills from EMS |
| IN | 5568 | String | Order acks from EMS |
| OUT | 5564 | String | Orders to EMS |
| OUT | 5571 | String | Position updates to PMS |
| OUT | 5567 | String | Trade records to persistence |
| OUT | 5572 | String | Audit events |

## Order State Machine

```
PendingNew → New → Acknowledged → PartiallyFilled → Filled
                 ↘ Rejected      ↘ Cancelled
```

**CRITICAL**: Always check `can_transition_to()` before state changes!

## Testing

```bash
cargo build --release -p oms
cargo test --release -p oms
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| Double fills | Dedup cache miss | Increase LRU size |
| Stuck orders | Invalid transition | Check state machine |
