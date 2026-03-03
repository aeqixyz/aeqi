# Pipeline: Data

## Services

| Service | Port | Purpose |
|---------|------|---------|
| **ingestion** | 9000 | WebSocket normalization from exchanges → Tick FlatBuffers |
| **aggregation** | 9001 | Tick → AVBB multi-volatility bars |
| **persistence** | 9002 | Batch writer to TimescaleDB |

## Data Flow

```
Exchange WebSocket APIs
        │
        ▼
┌───────────────────┐
│    INGESTION      │
│  (Port 9000)      │
│                   │
│  • Binance WS     │
│  • Bybit WS       │
│  • Hyperliquid WS │
│                   │
│  Parse JSON →     │
│  Normalize →      │
│  FlatBuffer       │
└─────────┬─────────┘
          │ ZMQ PUB 5555
          │ Binary topic: [market_key:8]
          │ Payload: Tick FlatBuffer
          ▼
┌───────────────────┐
│   AGGREGATION     │
│  (Port 9001)      │
│                   │
│  • Update state   │
│  • Volatility     │
│    thresholds     │
│  • Emit bars      │
└─────────┬─────────┘
          │ ZMQ PUB 5556
          │ Binary topic: [market_key:8][bar_key:4]
          │ Payload: Bar FlatBuffer
          ▼
┌───────────────────┐
│   PERSISTENCE     │
│  (Port 9002)      │
│                   │
│  • Batch writes   │
│  • TimescaleDB    │
│  • Compression    │
└───────────────────┘
          │
          ▼
     TimescaleDB
```

## ZMQ Topics

| Direction | Port | Topic Format | Size | Payload |
|-----------|------|--------------|------|---------|
| Ingestion → Aggregation | 5555 | `[market_key:8]` | 8 bytes | Tick FlatBuffer |
| Ingestion → Stream | 5555 | `[market_key:8]` | 8 bytes | Tick FlatBuffer |
| Aggregation → Feature | 5556 | `[market_key:8][bar_key:4]` | 12 bytes | Bar FlatBuffer |
| Aggregation → Persistence | 5556 | `[market_key:8][bar_key:4]` | 12 bytes | Bar FlatBuffer |

## Latency Targets

| Operation | Target | Typical | Notes |
|-----------|--------|---------|-------|
| JSON parse | <2.5μs | 1-2μs | simd-json |
| Tick publish | <500ns | 200ns | Binary topic |
| Bar processing | <5μs | 2-3μs | Per tick |
| Batch persist | <10ms | 5ms | 100-row batches |

## Key Patterns

### Ingestion: simd-json Zero-Copy Parsing

```rust
use simd_json::prelude::*;

fn parse_tick(json: &mut [u8]) -> Option<TickData> {
    // Zero-copy SIMD-accelerated parsing
    let value = simd_json::to_borrowed_value(json).ok()?;
    // ... extract fields
}
```

### Ingestion: Binary Topic Publishing

```rust
use keys::{format_tick_topic, TopicBuffer, TICK_TOPIC_SIZE};

fn publish_tick(pub_socket: &mut Publisher, market_key: i64, payload: &[u8]) {
    let mut topic = TopicBuffer::new();
    let topic_bytes = format_tick_topic(&mut topic, market_key);

    // [topic][space][payload]
    let mut msg = Vec::with_capacity(TICK_TOPIC_SIZE + 1 + payload.len());
    msg.extend_from_slice(topic_bytes);
    msg.push(b' ');
    msg.extend_from_slice(payload);

    pub_socket.send(msg);
}
```

### Aggregation: AVBB Bar Types

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

### Aggregation: State Management

```rust
use dashmap::DashMap;
use keys::CompositeKey;

struct AggregationState {
    // Per-symbol, per-bar-type state
    bars: DashMap<CompositeKey, BarBuilder>,
}

impl AggregationState {
    fn process_tick(&self, tick: TickData) {
        // Update all bar types for this symbol
        for bar_type in BAR_TYPES {
            let key = CompositeKey::new(tick.market_key, bar_type.key());
            self.bars
                .entry(key)
                .or_insert_with(|| BarBuilder::new(bar_type))
                .update(&tick);
        }
    }
}
```

### Persistence: Batch Writing

```rust
use tokio_postgres::Client;

async fn batch_insert(client: &Client, bars: &[BarData]) -> Result<(), Error> {
    // COPY is faster than INSERT for bulk data
    let copy_stmt = "COPY bars (market_key, bar_key, ...) FROM STDIN BINARY";
    let sink = client.copy_in(copy_stmt).await?;
    // ... write binary rows
}
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| High parse latency | Not using simd-json | Enable simd-json feature |
| Missing ticks | WS disconnection | Check reconnection metrics |
| Bar gaps | Tick gaps | Check ingestion throughput |
| Persistence backpressure | Slow DB | Increase batch size, check indexes |

## Required Crate Skills

Before modifying this pipeline, read:
1. `.claude/skills/crates/types.md` - TickData, BarData structs
2. `.claude/skills/crates/keys.md` - Topic formatting
3. `.claude/skills/crates/zmq_transport.md` - Pub/sub patterns
4. `.claude/skills/crates/ports.md` - Port constants

## Service-Specific Skills

- `.claude/skills/services/ingestion.md`
- `.claude/skills/services/aggregation.md`
- `.claude/skills/services/persistence.md`

## Monitoring

| Service | Key Metrics |
|---------|-------------|
| Ingestion | `ticks_received_total`, `parse_latency_ns`, `ws_reconnects` |
| Aggregation | `bars_emitted_total`, `process_latency_ns`, `active_symbols` |
| Persistence | `rows_written_total`, `batch_latency_ms`, `queue_depth` |

## Configuration

### Ingestion (`config/dev/ingestion.yaml`)
```yaml
venues:
  - binance
  - bybit
ws:
  reconnect_delay_ms: 1000
  max_reconnect_attempts: 10
zmq:
  pub_endpoint: "tcp://0.0.0.0:5555"
```

### Aggregation (`config/dev/aggregation.yaml`)
```yaml
bar_types:
  - type: PRBB
    threshold: 0.001
  - type: Time
    interval_seconds: 60
zmq:
  sub_endpoint: "tcp://127.0.0.1:5555"
  pub_endpoint: "tcp://0.0.0.0:5556"
```

### Persistence (`config/dev/persistence.yaml`)
```yaml
database:
  url: "postgresql://algo_dev@localhost/algostaking_dev"
  pool_size: 10
batch:
  size: 100
  flush_interval_ms: 1000
```
