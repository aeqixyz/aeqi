# Service: stream

## Required Reading
1. `.claude/skills/pipelines/gateway.md`
2. `.claude/skills/websocket-subscriptions.md`

## Purpose

High-performance WebSocket gateway with sub-microsecond latency for real-time market data.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point |
| `src/websocket.rs` | fastwebsockets integration |
| `src/channels.rs` | Channel subscription logic |
| `src/frame.rs` | Binary frame encoding |
| `src/worker.rs` | CPU-pinned worker threads |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `server.port` | `8081` | WebSocket port |
| `server.metrics_port` | `9011` | Metrics port |
| `performance.worker_threads` | `4` | Worker count |
| `performance.ring_buffer_size` | `65536` | SPSC buffer |
| `performance.cpu_affinity` | `[0,1,2,3]` | Core pinning |
| `rate_limit.connections_per_ip` | `10` | Max connections |
| `rate_limit.messages_per_second` | `1000` | Rate limit |

## ZMQ Connections (Input)

| Port | Topic | Data |
|------|-------|------|
| 5555 | Binary 8-byte | Ticks |
| 5556 | Binary 12-byte | Bars |
| 5557 | Binary 14-byte | Features |
| 5561 | Binary 14-byte | Signals |

## WebSocket Protocol

### Client → Server
```json
{"type": "subscribe", "channels": [{"market_key": 123, "bar_key": 456}]}
{"type": "unsubscribe", "channels": [...]}
{"type": "ping"}
```

### Server → Client (Binary)
```
Frame: [type:1][length:2][payload:N]

Types:
  1 = Tick
  2 = Bar
  3 = Feature
  4 = Signal
  10 = Snapshot
  255 = Error
```

## Performance Optimizations

- **fastwebsockets** - Minimal overhead WS library
- **CPU affinity** - Workers pinned to specific cores
- **SPSC ring buffers** - Lock-free producer-consumer
- **Binary frames** - No JSON parsing overhead
- **Object pooling** - Reuse frame buffers

## Testing

```bash
cargo build --release -p stream

# Load test
websocat wss://dev.app.algostaking.com/ws \
  -H "Authorization: Bearer <token>"
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| High latency | Queue backup | Increase ring_buffer_size |
| Dropped frames | Too many connections | Check rate limits |
