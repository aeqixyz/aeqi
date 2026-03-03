# Crate: types

## Purpose

Single source of truth for all FlatBuffer schemas and shared data structures used across AlgoStaking services. Provides zero-copy binary serialization for hot-path messaging and canonical Rust structs for FlatBuffer parsing.

## Public API

### Core Data Structs (Parsed from FlatBuffers)

| Type | Size | Description |
|------|------|-------------|
| `BarData` | 112 bytes, Copy | OHLCV bar with context (market_key, bar_key, bar_index) |
| `TickData` | 40 bytes, Copy | Market data tick (price, quantity, timestamp) |
| `FeatureData` | Clone | Feature vector with bar composition |
| `PredictionData` | Clone | Prediction (magnitude/retracement/horizon) with bar |
| `SignalData` | Clone | LTC-refined signal with prediction composition |

### FlatBuffer Modules

```rust
// Core hot-path types
use types::fb_tick_generated::fb_tick::{Tick, root_as_tick};
use types::fb_bar_generated::fb_bar::{Bar, root_as_bar};
use types::fb_features_generated::features::{FeatureVector, root_as_feature_vector};
use types::fb_prediction_generated::prediction::{Prediction, root_as_prediction};
use types::fb_signal_generated::algostaking::signal::{Signal};

// Registry types
use types::fb_asset_generated::registry::Asset;
use types::fb_venue_generated::registry::Venue;
use types::fb_bar_type_generated::registry::BarType;
```

### OMS/EMS Types

| Type | Description |
|------|-------------|
| `OrderSide` | Buy/Sell enum with conversion helpers |
| `OrderType` | Market/Limit enum |
| `OrderEvent` | Order from OMS to EMS |
| `FillEvent` | Fill from EMS to OMS |
| `OpenTrade` | 128-byte fixed format PMS->OMS intent |
| `ManagedOrder` | Full order state machine |
| `Position` | Open position tracking |
| `Account` | Trading account configuration |

## Canonical Usage

### Pattern 1: Parse FlatBuffer with Context from ZMQ Topic

Context fields (market_key, bar_key, bar_index) come from ZMQ topic bytes, NOT from FlatBuffer:

```rust
use types::BarData;
use keys::{parse_bar_topic_binary, BAR_TOPIC_SIZE};

fn handle_bar_message(msg: &ZmqMessage) -> Option<BarData> {
    let data = msg.get(0)?;
    let bytes: &[u8] = data.as_ref();

    // Extract context from binary topic (first 12 bytes)
    let (market_key, bar_key) = parse_bar_topic_binary(&bytes[..BAR_TOPIC_SIZE]);
    let bar_index = extract_bar_index_from_somewhere(); // Often from state

    // Skip topic + space, parse FlatBuffer payload
    let payload = &bytes[BAR_TOPIC_SIZE + 1..];
    let bar_fb = unsafe { flatbuffers::root_unchecked::<Bar>(payload) };

    // Context passed to from_fb(), not in FlatBuffer
    Some(BarData::from_fb(&bar_fb, market_key, bar_key, bar_index))
}
```

### Pattern 2: Use from_fb_nested() for Composed Types

When bar is nested inside another message:

```rust
use types::{SignalData, PredictionData};

fn handle_signal(signal_fb: &Signal) -> Option<SignalData> {
    // SignalData::from_fb handles nested PredictionData extraction
    SignalData::from_fb(signal_fb)
}

fn handle_prediction(pred_fb: &Prediction) -> Option<PredictionData> {
    // from_fb extracts nested bar using from_fb_nested
    PredictionData::from_fb(pred_fb)
}
```

### Pattern 3: Zero-Copy Hot Path with Prelude

```rust
use types::prelude::*;  // All common types

// Zero-copy FlatBuffer access (trusted internal source)
let tick = unsafe { root_as_tick(payload).unwrap_unchecked() };
let tick_data = TickData::from_fb(&tick);

// tick_data is Copy, no allocation
process_tick(tick_data);
```

### Pattern 4: OMS Order State Machine

```rust
use types::{ManagedOrder, ManagedOrderStatus, OrderSide};

let mut order = ManagedOrder::from_target(order_id, &target, symbol, price, ts);

// State transitions
assert!(order.status.can_transition_to(ManagedOrderStatus::New));
order.status = ManagedOrderStatus::New;

// Apply fills
if order.status.can_fill() {
    order.apply_fill(fill_qty, fill_price, timestamp_us);
}
```

## Anti-Patterns

### DON'T: Include Context in FlatBuffer

```rust
// WRONG: Redundant - context is already in ZMQ topic
let bar = BarArgs {
    market_key: 123,  // DON'T include - comes from topic
    bar_key: 456,     // DON'T include - comes from topic
    open: 50000.0,
    // ...
};
```

### DON'T: Use Deprecated Type Aliases

```rust
// WRONG: Deprecated aliases
use types::TradeData;  // Use TickData instead
use types::TensorData; // Use PredictionData instead
```

### DON'T: Clone Copy Types

```rust
// WRONG: Unnecessary clone
let tick_copy = tick_data.clone();  // TickData is Copy, just assign

// RIGHT
let tick_copy = tick_data;  // Copy by value
```

### DON'T: Allocate in Hot Path FlatBuffer Parsing

```rust
// WRONG: String allocation in hot path
let venue = signal.venue().unwrap().to_string();

// RIGHT: Keep &str reference where possible
let venue: &str = signal.venue().unwrap();
```

## Violation Detection

```bash
# Find uses of deprecated TradeData alias
rg "TradeData" --type rust services/

# Find uses of deprecated TensorData alias
rg "TensorData" --type rust services/

# Find potential redundant context in FlatBuffer construction
rg "BarArgs\s*\{" -A 5 --type rust | grep -E "market_key|bar_key"

# Find clone() on Copy types
rg "\.(clone|to_owned)\(\)" --type rust services/ | grep -E "TickData|BarData"
```

## Migration Guide

### From TradeData to TickData

```rust
// Before
use types::TradeData;
let trade = TradeData::from_fb(&fb);

// After
use types::TickData;
let tick = TickData::from_fb(&fb);
```

### From TensorData to PredictionData

```rust
// Before
use types::TensorData;
let tensor = TensorData::from_tensor_fb(&fb);

// After
use types::PredictionData;
let prediction = PredictionData::from_fb(&fb);  // Or from_tensor_fb for legacy
```

## Cross-References

- **Depends on:** `flatbuffers` crate
- **Used by:** All services (ingestion, aggregation, feature, prediction, signal, pms, oms, ems, stream)
- **Related skills:** `keys.md` (topic parsing), `zmq_transport.md` (message handling)

## Schema Files

All FlatBuffer schemas in `crates/types/schemas/`:

| Schema | Purpose |
|--------|---------|
| `fb_tick.fbs` | Market data ticks |
| `fb_bar.fbs` | OHLCV bars |
| `fb_features.fbs` | Feature vectors |
| `fb_prediction.fbs` | Trading predictions |
| `fb_signal.fbs` | LTC signals |
| `fb_order_event.fbs` | OMS orders |
| `fb_fill_event.fbs` | EMS fills |
