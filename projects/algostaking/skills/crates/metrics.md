# Crate: metrics

## Purpose

HFT-compliant Prometheus metrics for all services. Provides zero-allocation recording in hot paths with lock-free atomic operations.

## Public API

### Metric Types

```rust
use metrics::{
    Counter,           // Monotonically increasing counter
    Gauge,             // Value that can go up/down (i64)
    UGauge,            // Unsigned gauge (u64)
    Histogram,         // Distribution with percentile buckets
    LatencyTracker,    // Convenience for timing operations
    LatencyBuckets,    // Pre-defined histogram buckets
};
```

### Registry and Export

```rust
use metrics::{
    MetricsRegistry,   // Collects all metrics for export
    PrometheusExport,  // Trait for exporting metrics
    format_counter,    // Format counter for Prometheus
    format_counter_labeled,  // Counter with labels
    format_gauge,      // Format gauge for Prometheus
    start_server,      // Start HTTP metrics server
};
```

## Canonical Usage

### Pattern 1: Define Static Metrics

```rust
use metrics::{Counter, Gauge, Histogram, LatencyBuckets};
use std::sync::atomic::Ordering;

// Static metrics (zero-alloc recording)
static TRADES_RECEIVED: Counter = Counter::new();
static ACTIVE_SYMBOLS: Gauge = Gauge::new();
static PARSE_LATENCY: Histogram = Histogram::with_buckets(LatencyBuckets::MICROSECONDS);
```

### Pattern 2: Record in Hot Path

```rust
// Counter: increment (1ns)
TRADES_RECEIVED.inc();
TRADES_RECEIVED.add(batch_size);

// Gauge: set current value (1ns)
ACTIVE_SYMBOLS.set(symbols.len() as i64);
ACTIVE_SYMBOLS.inc();  // +1
ACTIVE_SYMBOLS.dec();  // -1

// Histogram: record observation (5ns)
PARSE_LATENCY.record(elapsed_ns);
```

### Pattern 3: Time Operations with LatencyTracker

```rust
use metrics::LatencyTracker;

static PROCESS_LATENCY: LatencyTracker = LatencyTracker::new();

fn process_bar(bar: &BarData) {
    let _guard = PROCESS_LATENCY.start();  // Starts timing
    // ... processing ...
    // Guard dropped, latency recorded automatically
}
```

### Pattern 4: Create Registry and Export

```rust
use metrics::{MetricsRegistry, format_counter, format_gauge, start_server};
use ports::METRICS_AGGREGATION;

async fn run_metrics_server() {
    let registry = MetricsRegistry::new("aggregation");

    start_server(METRICS_AGGREGATION, registry).await;
}

impl PrometheusExport for AggregationService {
    fn export(&self, registry: &mut MetricsRegistry) {
        registry.add_metric(format_counter(
            "trades_received_total",
            TRADES_RECEIVED.get(),
            "Total trades received from ingestion",
        ));

        registry.add_metric(format_gauge(
            "active_symbols",
            ACTIVE_SYMBOLS.get(),
            "Number of symbols being aggregated",
        ));

        // Histogram exports percentiles automatically
        PARSE_LATENCY.export(registry, "parse_latency_ns");
    }
}
```

### Pattern 5: Use Pre-defined Buckets

```rust
use metrics::{Histogram, LatencyBuckets};

// Nanosecond buckets: 100ns, 500ns, 1us, 5us, 10us, 50us, 100us, 500us, 1ms
static HOT_PATH_LATENCY: Histogram = Histogram::with_buckets(LatencyBuckets::NANOSECONDS);

// Microsecond buckets: 1us, 5us, 10us, 50us, 100us, 500us, 1ms, 5ms, 10ms
static COLD_PATH_LATENCY: Histogram = Histogram::with_buckets(LatencyBuckets::MICROSECONDS);

// Millisecond buckets: 1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s, 10s
static API_LATENCY: Histogram = Histogram::with_buckets(LatencyBuckets::MILLISECONDS);
```

### Pattern 6: Labeled Counters

```rust
use metrics::format_counter_labeled;

fn export_per_venue_metrics(registry: &mut MetricsRegistry) {
    for (venue, count) in venue_counts.iter() {
        registry.add_metric(format_counter_labeled(
            "trades_by_venue_total",
            *count,
            &[("venue", venue)],
            "Trades received per venue",
        ));
    }
}
```

## Anti-Patterns

### DON'T: Use Mutex in Hot Path

```rust
// WRONG: Mutex contention
static COUNTER: Mutex<u64> = Mutex::new(0);
fn record() {
    *COUNTER.lock().unwrap() += 1;  // Slow!
}

// RIGHT: Lock-free atomics
static COUNTER: Counter = Counter::new();
fn record() {
    COUNTER.inc();  // Lock-free
}
```

### DON'T: Allocate Strings in Recording

```rust
// WRONG: String allocation per record
fn record_trade(symbol: &str) {
    let key = format!("trades_{}", symbol);  // Allocates!
    counters.get(&key).inc();
}

// RIGHT: Pre-defined metrics
static BTC_TRADES: Counter = Counter::new();
static ETH_TRADES: Counter = Counter::new();
fn record_trade(market_key: i64) {
    match market_key.base_asset_id() {
        100 => BTC_TRADES.inc(),
        101 => ETH_TRADES.inc(),
        _ => {}
    }
}
```

### DON'T: Create Metrics Dynamically in Hot Path

```rust
// WRONG: Dynamic metric creation
fn process(symbol: &str) {
    let counter = Counter::new();  // Created each call!
    counter.inc();
}

// RIGHT: Static metrics
static COUNTER: Counter = Counter::new();
fn process(_symbol: &str) {
    COUNTER.inc();
}
```

### DON'T: Skip Histogram Bucket Selection

```rust
// WRONG: Default buckets may not fit your latency profile
static LATENCY: Histogram = Histogram::new();  // Generic buckets

// RIGHT: Choose appropriate buckets for your use case
static LATENCY: Histogram = Histogram::with_buckets(LatencyBuckets::NANOSECONDS);
```

## Violation Detection

```bash
# Find Mutex usage in services (potential hot-path contention)
rg "Mutex<" --type rust services/ | grep -v "config\|state\|setup"

# Find format! in hot paths (string allocation)
rg "format!\(" --type rust services/*/src/*.rs | grep -v "log::\|error!\|warn!\|info!"

# Find dynamic metric creation
rg "Counter::new\(\)|Gauge::new\(\)" --type rust services/ | grep -v "static\|lazy_static"

# Find missing metrics export
for svc in services/*/*; do
    if [ -f "$svc/src/main.rs" ]; then
        if ! grep -q "PrometheusExport\|start_server" "$svc/src/main.rs"; then
            echo "Missing metrics export: $svc"
        fi
    fi
done
```

## Migration Guide

### From prometheus crate to metrics

```rust
// Before (prometheus crate - allocates, slower)
use prometheus::{Counter, register_counter};
let counter = register_counter!("my_counter", "help").unwrap();
counter.inc();

// After (our metrics crate - zero-alloc)
use metrics::Counter;
static MY_COUNTER: Counter = Counter::new();
MY_COUNTER.inc();
```

## Cross-References

- **Depends on:** `ports` (metrics port constants)
- **Used by:** All services for observability
- **Related skills:** `monitoring.md` (Prometheus/Grafana setup)
- **Code location:** `crates/metrics/src/`

## Key Files

| File | Purpose |
|------|---------|
| `counter.rs` | Counter with AtomicU64 |
| `gauge.rs` | Gauge with AtomicI64, UGauge with AtomicU64 |
| `histogram.rs` | Histogram with bucket distribution |
| `latency.rs` | LatencyTracker with RAII guard |
| `registry.rs` | MetricsRegistry and format functions |
| `server.rs` | Axum-based HTTP server for /metrics |

## Performance Characteristics

| Operation | Latency | Allocations |
|-----------|---------|-------------|
| Counter.inc() | ~1ns | 0 |
| Gauge.set() | ~1ns | 0 |
| Histogram.record() | ~5ns | 0 |
| Registry export | ~100us | O(metrics) |
