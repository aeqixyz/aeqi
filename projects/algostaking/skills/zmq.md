# ZeroMQ Messaging

## Overview

Services communicate via ZeroMQ pub/sub with FlatBuffers serialization.
All connections use `127.0.0.1` (not Docker hostnames).

**Hot-path topics use BINARY format** for zero-allocation. See `keys-packing.md` for full details.

## Port Map

### Data Pipeline (Binary Topics)
| Port | Publisher | Subscribers | Topic Size | Format |
|------|-----------|-------------|------------|--------|
| 5555 | ingestion | aggregation, stream | 8 bytes | `[market_key:i64]` |
| 5556 | aggregation | persistence, feature, stream | 12 bytes | `[market_key:i64][bar_key:i32]` |

### Strategy Pipeline (Binary Topics)
| Port | Publisher | Subscribers | Topic Size | Format |
|------|-----------|-------------|------------|--------|
| 5557 | feature | prediction, stream | 14 bytes | `[market_key:i64][bar_key:i32][feature_id:u16]` |
| 5558 | prediction | signal, stream | 14 bytes | `[market_key:i64][bar_key:i32][feature_id:u16]` |
| 5561 | signal | pms | 14 bytes | `[market_key:i64][bar_key:i32][feature_id:u16]` |

**Note:** `feature_id` is also called `schema_id` in some contexts - they're the same value.

### Trading Pipeline
| Port | Publisher | Subscribers | Topic Pattern |
|------|-----------|-------------|---------------|
| 5564 | oms | ems | Orders |
| 5565 | ems | oms | Fills |
| 5566 | ems | pms | Account state |
| 5567 | oms | (audit) | Trades |
| 5568 | ems | oms | Acks |
| 5570 | pms | oms | Position intents |
| 5571 | oms | pms | Position updates |
| 5572 | pms | persistence | Portfolio snapshots |

### Configuration
| Port | Publisher | Subscribers | Topic Pattern |
|------|-----------|-------------|---------------|
| 5550 | configuration | all services | `config.registry.*` |
| 5554 | configuration | services | `perp_market.*` |

## Connection Patterns

### Publisher (bind)
```rust
// Service that produces data binds to port
let publisher = zmq_transport::Publisher::bind("tcp://0.0.0.0:5555")?;
```

### Subscriber (connect)
```rust
// Services that consume data connect to publisher
let subscriber = zmq_transport::Subscriber::connect("tcp://127.0.0.1:5555")?;
subscriber.subscribe("tick.")?;  // Subscribe to topic prefix
```

## Config File Endpoints

### Bind endpoints (use `0.0.0.0`)
```yaml
# For publishers - accept connections from any interface
tick_endpoint: "tcp://0.0.0.0:5555"
```

### Connect endpoints (use `127.0.0.1`)
```yaml
# For subscribers - connect to localhost
tick_endpoint: "tcp://127.0.0.1:5555"
```

## Common Issues

### "failed to lookup address"
- Using Docker hostname like `tcp://data_ingestion_service:5555`
- Fix: Change to `tcp://127.0.0.1:5555`

### "Address already in use"
- Two services trying to bind same port
- Check: `ss -tlnp | grep 5555`

### "Network Error: Name or service not known"
- Using `tcp://*:5555` for bind (doesn't work on this system)
- Fix: Change to `tcp://0.0.0.0:5555`

## Data Flow

```
┌──────────┐     5555      ┌─────────────┐     5556      ┌─────────────┐
│ Ingestion│──────────────▶│ Aggregation │──────────────▶│ Persistence │
└──────────┘   (ticks)     └─────────────┘   (bars)      └─────────────┘
                                  │
                                  │ 5556
                                  ▼
                           ┌─────────────┐     5557      ┌────────────┐
                           │   Feature   │──────────────▶│ Prediction │
                           └─────────────┘  (features)   └────────────┘
                                                               │
                                                               │ 5558
                                                               ▼
┌─────┐     5570      ┌─────┐     5564      ┌─────┐     ┌──────────┐
│ PMS │──────────────▶│ OMS │──────────────▶│ EMS │◀────│  Signal  │
└─────┘   (intents)   └─────┘   (orders)    └─────┘     └──────────┘
    ▲                     │                     │              │
    │                     │ 5571               │ 5565         │ 5561
    │                     └─────────────────────┘              │
    └──────────────────────────────────────────────────────────┘
```

## Topic Size Constants

Defined in `crates/keys/src/topic.rs`:
```rust
pub const TICK_TOPIC_SIZE: usize = 8;      // market_key only
pub const BAR_TOPIC_SIZE: usize = 12;      // market_key + bar_key
pub const FEATURE_TOPIC_SIZE: usize = 14;  // market_key + bar_key + feature_id
pub const PREDICTION_TOPIC_SIZE: usize = 14;
```

## Topic Functions

```rust
use keys::{MarketKey, BarKey, format_tick_topic, format_bar_topic_binary, format_feature_topic_binary};

// 8-byte tick topic
let tick_topic = format_tick_topic(market_key);

// 12-byte bar topic
let bar_topic = format_bar_topic_binary(market_key, bar_key);

// 14-byte feature/prediction/signal topic
let feature_topic = format_feature_topic_binary(market_key, bar_key, feature_id);
```

## Port Constants

Defined in `crates/ports/src/lib.rs`:
```rust
pub const TICK_PUB_PORT: u16 = 5555;
pub const BAR_PUB_PORT: u16 = 5556;
// etc.
```
