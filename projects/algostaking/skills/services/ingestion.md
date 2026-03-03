# Service: ingestion

## Required Reading
1. `.claude/skills/pipelines/data.md`
2. `.claude/skills/crates/types.md` - TickData
3. `.claude/skills/crates/keys.md` - format_tick_topic
4. `.claude/skills/crates/zmq_transport.md`

## Purpose

Connects to exchange WebSocket APIs, normalizes market data to TickData, and publishes via ZMQ.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, service orchestration |
| `src/venues/` | Exchange-specific adapters (binance.rs, bybit.rs, etc.) |
| `src/normalizer.rs` | Tick normalization logic |
| `src/publisher.rs` | ZMQ publishing with binary topics |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `venues` | `[binance, bybit]` | Enabled exchanges |
| `ws.reconnect_delay_ms` | `1000` | Reconnection backoff |
| `ws.max_reconnect_attempts` | `10` | Max reconnection tries |
| `zmq.pub_endpoint` | `tcp://0.0.0.0:5555` | ZMQ PUB socket |

## ZMQ Connections

| Direction | Port | Topic | Description |
|-----------|------|-------|-------------|
| OUT | 5555 | Binary 8-byte market_key | Tick FlatBuffers |
| OUT | 5554 | String | Perp market metadata |
| IN/OUT | 5553 | REQ/REP | Backfill requests |

## Adding a New Venue

1. Create `venues/<exchange>.rs`
2. Implement `VenueAdapter` trait:
   ```rust
   #[async_trait]
   pub trait VenueAdapter: Send + Sync {
       async fn connect(&mut self) -> Result<()>;
       async fn subscribe(&mut self, markets: &[MarketKey]) -> Result<()>;
       async fn recv(&mut self) -> Option<RawMessage>;
       fn parse(&self, msg: &RawMessage) -> Option<TickData>;
   }
   ```
3. Add to venue registry in config
4. Map exchange symbols to MarketKey

## Testing

```bash
# Build
cargo build --release -p ingestion

# Run with dev config
CONFIG_PATH=config/dev.yaml ./target/release/ingestion

# Check metrics
curl http://localhost:9000/metrics | grep ticks_received
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| No ticks | WS not connected | Check venue logs, API key |
| Parse errors | JSON format changed | Update venue parser |
| High latency | Not using simd-json | Enable simd-json feature |
