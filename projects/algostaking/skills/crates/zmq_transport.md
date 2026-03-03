# Crate: zmq_transport

## Purpose

Resilient ZMQ subscriber and publisher for HFT services. Eliminates ~100 lines of boilerplate across all services while providing auto-reconnect with exponential backoff.

## Public API

### Core Types

```rust
use zmq_transport::{
    // Subscriber
    ResilientSubscriber,
    SubscriberConfig,

    // Publisher
    ResilientPublisher,
    PublisherConfig,

    // Parser trait
    MessageParser,
    RawParser,  // Returns raw bytes

    // Metrics
    ZmqMetrics,

    // Re-exported for parsers
    ZmqMessage,

    // Errors
    ZmqError,
};
```

### ZmqMetrics

```rust
pub struct ZmqMetrics {
    pub messages_received: AtomicU64,
    pub messages_sent: AtomicU64,
    pub parse_errors: AtomicU64,
    pub reconnections: AtomicU64,
    pub bytes_received: AtomicU64,
    pub bytes_sent: AtomicU64,
}
```

### MessageParser Trait

```rust
pub trait MessageParser: Send + Sync {
    type Output: Send;
    fn parse(&self, msg: &ZmqMessage) -> Option<Self::Output>;
}
```

## Canonical Usage

### Pattern 1: Define Service-Specific Parser

```rust
use zmq_transport::{MessageParser, ZmqMessage};
use types::BarData;
use keys::{parse_bar_topic_binary, BAR_TOPIC_SIZE};

struct BarParser;

impl MessageParser for BarParser {
    type Output = BarData;

    fn parse(&self, msg: &ZmqMessage) -> Option<Self::Output> {
        let data = msg.get(0)?;
        let bytes: &[u8] = data.as_ref();

        // Binary topic: [12 bytes topic][space][payload]
        if bytes.len() <= BAR_TOPIC_SIZE || bytes[BAR_TOPIC_SIZE] != b' ' {
            return None;
        }

        // Parse topic (zero-alloc)
        let (market_key, bar_key) = parse_bar_topic_binary(&bytes[..BAR_TOPIC_SIZE]);
        let payload = &bytes[BAR_TOPIC_SIZE + 1..];

        // Zero-copy FlatBuffer (trusted internal source)
        let bar_fb = unsafe { flatbuffers::root_unchecked::<Bar>(payload) };

        Some(BarData::from_fb(&bar_fb, market_key, bar_key, 0))
    }
}
```

### Pattern 2: Create Subscriber (Replaces ~100 Lines)

```rust
use zmq_transport::{ResilientSubscriber, ZmqMetrics};
use ports::AGGREGATION_BARS_CONNECT;
use std::sync::Arc;

async fn run_subscriber() -> Result<(), Box<dyn std::error::Error>> {
    let metrics = Arc::new(ZmqMetrics::new());

    // Empty string = subscribe to all topics
    let mut sub = ResilientSubscriber::new(
        AGGREGATION_BARS_CONNECT,
        &[""],
        BarParser,
        metrics.clone(),
    ).await?;

    // Auto-reconnects on failure!
    loop {
        if let Some(bar) = sub.recv().await {
            process_bar(bar);
        }
    }
}
```

### Pattern 3: Selective Topic Subscription

```rust
use keys::{format_tick_topic, TopicBuffer};

// Subscribe to specific market only
let mut topic_buf = TopicBuffer::new();
let btc_topic = format_tick_topic(&mut topic_buf, btc_market_key);

let mut sub = ResilientSubscriber::new(
    INGESTION_TRADES_CONNECT,
    &[std::str::from_utf8(btc_topic).unwrap()],  // Only BTC
    TickParser,
    metrics,
).await?;
```

### Pattern 4: Publisher with Worker Pattern

```rust
use zmq_transport::{ResilientPublisher, ZmqMetrics};
use tokio::sync::mpsc;
use ports::AGGREGATION_BARS_BIND;

async fn run_publisher() -> Result<(), Box<dyn std::error::Error>> {
    let metrics = Arc::new(ZmqMetrics::new());
    let mut publisher = ResilientPublisher::new(
        AGGREGATION_BARS_BIND,
        metrics.clone(),
    ).await?;

    // Bounded channel for backpressure
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(1000);

    // Worker thread: handles ZMQ sends
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = publisher.send(&msg).await {
                log::error!("Publish failed: {}", e);
            }
        }
    });

    // Hot path: non-blocking try_send (sheds load if full)
    // let _ = tx.try_send(serialized_message);

    Ok(())
}
```

### Pattern 5: Expose Metrics to Prometheus

```rust
use metrics::{MetricsRegistry, format_counter};

fn export_zmq_metrics(zmq: &ZmqMetrics, registry: &mut MetricsRegistry) {
    registry.add_metric(format_counter(
        "zmq_messages_received_total",
        zmq.messages_received.load(Ordering::Relaxed),
        "Total ZMQ messages received",
    ));
    registry.add_metric(format_counter(
        "zmq_reconnections_total",
        zmq.reconnections.load(Ordering::Relaxed),
        "Total ZMQ reconnections",
    ));
}
```

## Anti-Patterns

### DON'T: Implement Reconnection Manually

```rust
// WRONG: Manual reconnection (error-prone)
loop {
    match zmq_socket.recv() {
        Ok(msg) => process(msg),
        Err(_) => {
            sleep(Duration::from_secs(1));
            zmq_socket = create_new_socket()?;  // Boilerplate!
        }
    }
}

// RIGHT: ResilientSubscriber handles reconnection
let mut sub = ResilientSubscriber::new(endpoint, topics, parser, metrics).await?;
loop {
    if let Some(msg) = sub.recv().await {
        process(msg);
    }
}
```

### DON'T: Allocate in Parser Hot Path

```rust
// WRONG: String allocation
impl MessageParser for BadParser {
    type Output = String;
    fn parse(&self, msg: &ZmqMessage) -> Option<String> {
        Some(String::from_utf8_lossy(msg.get(0)?).to_string())  // Allocates!
    }
}

// RIGHT: Zero-copy parsing
impl MessageParser for GoodParser {
    type Output = BarData;  // Copy type, no allocation
    fn parse(&self, msg: &ZmqMessage) -> Option<BarData> {
        // ... parse without allocation
    }
}
```

### DON'T: Block in recv() Callback

```rust
// WRONG: Blocking I/O in hot path
loop {
    if let Some(bar) = sub.recv().await {
        db.insert(&bar).await?;  // Blocks on DB!
    }
}

// RIGHT: Use channel + worker
let (tx, rx) = mpsc::channel(1000);
loop {
    if let Some(bar) = sub.recv().await {
        let _ = tx.try_send(bar);  // Non-blocking
    }
}
// Worker persists asynchronously
```

### DON'T: Ignore Metrics

```rust
// WRONG: No observability
let sub = ResilientSubscriber::new(endpoint, topics, parser, Arc::new(ZmqMetrics::new())).await?;
// Metrics created but never exported!

// RIGHT: Export to Prometheus
let metrics = Arc::new(ZmqMetrics::new());
let sub = ResilientSubscriber::new(endpoint, topics, parser, metrics.clone()).await?;
// ... in metrics handler
export_zmq_metrics(&metrics, &mut registry);
```

## Violation Detection

```bash
# Find manual reconnection loops
rg "reconnect|Reconnect" --type rust services/ | grep -v zmq_transport

# Find raw zeromq socket usage (should use ResilientSubscriber)
rg "zeromq::(Sub|Pub)Socket" --type rust services/

# Find blocking in hot paths
rg "\.await\?" --type rust services/ -A 2 | grep -E "db\.|pool\.|insert|update"

# Find unused metrics
rg "ZmqMetrics::new\(\)" --type rust -A 5 | grep -v "export\|prometheus\|registry"
```

## Migration Guide

### From Raw ZMQ to ResilientSubscriber

```rust
// Before (~100 lines)
let ctx = zeromq::Context::new();
let mut socket = ctx.socket(zeromq::SUB)?;
socket.connect(endpoint)?;
socket.set_subscribe("")?;
loop {
    match socket.recv() {
        Ok(msg) => {
            // Manual parsing...
            // Manual error handling...
        }
        Err(e) => {
            // Manual reconnection...
            std::thread::sleep(Duration::from_secs(1));
            socket = ctx.socket(zeromq::SUB)?;
            socket.connect(endpoint)?;
        }
    }
}

// After (~15 lines)
let mut sub = ResilientSubscriber::new(endpoint, &[""], MyParser, metrics).await?;
loop {
    if let Some(parsed) = sub.recv().await {
        process(parsed);
    }
}
```

## Cross-References

- **Depends on:** `ports` (endpoint constants), `keys` (topic parsing)
- **Used by:** All services with ZMQ pub/sub
- **Related skills:** `types.md` (FlatBuffer parsing in parsers)
- **Code location:** `crates/zmq_transport/src/`

## Key Files

| File | Purpose |
|------|---------|
| `subscriber.rs` | ResilientSubscriber with auto-reconnect |
| `publisher.rs` | ResilientPublisher with auto-reconnect |
| `parser.rs` | MessageParser trait definition |
| `metrics.rs` | ZmqMetrics with atomic counters |
| `error.rs` | ZmqError types |
