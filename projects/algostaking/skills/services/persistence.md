# Service: persistence

## Required Reading
1. `.claude/skills/pipelines/data.md`
2. `.claude/skills/crates/types.md` - BarData
3. `.claude/skills/database.md`

## Purpose

Batch writes bars and ticks to TimescaleDB for historical analysis and backtesting.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, worker setup |
| `src/writer.rs` | Batch insert logic |
| `src/schema.rs` | Table definitions |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `database.url` | env `DATABASE_URL` | PostgreSQL connection |
| `database.pool_size` | `10` | Connection pool size |
| `batch.size` | `100` | Rows per batch |
| `batch.flush_interval_ms` | `1000` | Max wait before flush |
| `zmq.sub_endpoint` | `tcp://127.0.0.1:5556` | Bar input |

## ZMQ Connections

| Direction | Port | Topic | Description |
|-----------|------|-------|-------------|
| IN | 5556 | Binary 12-byte | Bars from aggregation |
| IN | 5567 | String | Trade records from OMS |

## Database Tables

| Table | Purpose |
|-------|---------|
| `bars` | OHLCV bar data (hypertable) |
| `ticks` | Raw tick data (hypertable) |
| `trades` | Completed trade records |

## Testing

```bash
cargo build --release -p persistence
DATABASE_URL=postgresql://algo_dev@localhost/algostaking_dev \
  ./target/release/persistence
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| Slow inserts | No batching | Increase batch_size |
| Connection errors | Pool exhausted | Increase pool_size |
| Disk full | No compression | Enable TimescaleDB compression |
