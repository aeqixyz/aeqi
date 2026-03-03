# Crate: keys

## Purpose

Zero-allocation key packing for cache-friendly routing and ZMQ topic matching. All keys are designed for fast hash operations and binary topic formatting.

## Public API

### Key Types

| Type | Size | Composition |
|------|------|-------------|
| `MarketKey` | i64 (8 bytes) | `[inst_type:16\|base:16\|quote:16\|venue:16]` |
| `BarKey` | i32 (4 bytes) | `[bar_type:16\|variant:16]` |
| `StrategyKey` | 14 bytes | `market_key + bar_key + feature_id` |
| `ModelKey` | u64 | `[bar_type:32\|feature_id:32]` |
| `FnoHeadKey` | u64 | `[bar_type:16\|base_asset:16]` (venue-agnostic) |
| `LtcHeadKey` | 16 bytes | `model_key + market_key` (venue-specific) |
| `CompositeKey` | 12 bytes | `market_key + bar_key` (for HashMap) |

### ID Types (Use Instead of Raw Primitives)

```rust
use keys::{
    // Market data IDs
    AssetId, BarTypeId, FeatureId, InstrumentTypeId, VariantId, VenueId,
    // Trading IDs
    AccountId, FundId, OrderId, TradeId,
};
```

### Topic Functions

```rust
use keys::{
    // Tick topics (8 bytes)
    format_tick_topic, parse_tick_topic, TICK_TOPIC_SIZE,
    // Bar topics (12 bytes)
    format_bar_topic_binary, parse_bar_topic_binary, BAR_TOPIC_SIZE,
    // Feature topics (14 bytes)
    format_feature_topic_binary, parse_feature_topic_binary, FEATURE_TOPIC_SIZE,
    // Prediction topics (14 bytes)
    format_prediction_topic_binary, parse_prediction_topic_binary, PREDICTION_TOPIC_SIZE,
};
```

### Topic Size Constants

```rust
pub const TICK_TOPIC_SIZE: usize = 8;       // market_key only
pub const BAR_TOPIC_SIZE: usize = 12;       // market_key + bar_key
pub const FEATURE_TOPIC_SIZE: usize = 14;   // market_key + bar_key + feature_id
pub const PREDICTION_TOPIC_SIZE: usize = 14; // Same as feature
```

## Canonical Usage

### Pattern 1: Create and Pack Keys

```rust
use keys::{MarketKey, BarKey, StrategyKey};

// Create market key: BTC-USDT perp on Binance
let market = MarketKey::new(
    2,    // instrument_type_id (perp-linear)
    100,  // base_asset_id (BTC)
    200,  // quote_asset_id (USDT)
    1,    // venue_id (Binance)
);
let raw: i64 = market.as_raw();  // Packed i64 for storage

// Create bar key: 1-minute time bar
let bar = BarKey::new(10, 1);  // bar_type=10 (time), variant=1 (1m)
let raw: i32 = bar.as_raw();

// Create strategy key (combines all)
let strategy = StrategyKey::new(market, bar, 5);  // feature_id = 5
```

### Pattern 2: Extract Components from Packed Key

```rust
use keys::MarketKey;

let market = MarketKey::from_raw(packed_i64);
let venue_id: u16 = market.venue_id();
let base_asset_id: u16 = market.base_asset_id();
let quote_asset_id: u16 = market.quote_asset_id();
let instrument_type_id: u16 = market.instrument_type_id();
```

### Pattern 3: Format Binary ZMQ Topics

```rust
use keys::{format_bar_topic_binary, BAR_TOPIC_SIZE, TopicBuffer};

// Stack-allocated topic buffer (no heap allocation)
let mut topic = TopicBuffer::new();
let topic_bytes = format_bar_topic_binary(&mut topic, market_key, bar_key);
// topic_bytes is &[u8; 12] - ready for ZMQ send
```

### Pattern 4: Parse Binary Topics from ZMQ

```rust
use keys::{parse_bar_topic_binary, parse_feature_topic_binary, BAR_TOPIC_SIZE};

fn handle_message(msg: &[u8]) {
    // Bar message (12-byte topic)
    let (market_key, bar_key) = parse_bar_topic_binary(&msg[..BAR_TOPIC_SIZE]);

    // Feature message (14-byte topic)
    let (market_key, bar_key, feature_id) = parse_feature_topic_binary(&msg[..14]);
}
```

### Pattern 5: Use CompositeKey for HashMap Routing

```rust
use keys::CompositeKey;
use std::collections::HashMap;

let mut state: HashMap<CompositeKey, BarState> = HashMap::new();

// Create key from market + bar
let key = CompositeKey::new(market_key, bar_key);
state.insert(key, bar_state);

// Lookup
if let Some(state) = state.get(&key) {
    // ...
}
```

### Pattern 6: Head Keys for Model Routing

```rust
use keys::{FnoHeadKey, LtcHeadKey, ModelKey, StrategyKey};

// FNO: Venue-agnostic (shares heads across venues)
let fno_head = FnoHeadKey::from_strategy(&strategy);
// Same head for BTC on Binance, Bybit, etc.

// LTC: Venue-specific (separate hidden state per venue)
let ltc_head = LtcHeadKey::from_strategy(&strategy);
// Different head per venue
```

## Anti-Patterns

### DON'T: Use Raw Integers Without Type Safety

```rust
// WRONG: Raw i64 loses semantic meaning
fn process(market_key: i64, bar_key: i64) { }  // bar_key should be i32!

// RIGHT: Use typed wrappers
fn process(market: MarketKey, bar: BarKey) { }
```

### DON'T: Allocate Strings for Topics

```rust
// WRONG: String allocation in hot path
let topic = format!("{}:{}", market_key, bar_key);

// RIGHT: Binary topics with stack buffer
let mut buf = TopicBuffer::new();
let topic = format_bar_topic_binary(&mut buf, market_key, bar_key);
```

### DON'T: Parse Topics Repeatedly

```rust
// WRONG: Parse same topic multiple times
let market1 = parse_tick_topic(&bytes[..8]);
let market2 = parse_tick_topic(&bytes[..8]);  // Redundant

// RIGHT: Parse once, store result
let market_key = parse_tick_topic(&bytes[..8]);
use_key(market_key);
also_use_key(market_key);
```

### DON'T: Mix Deprecated and New APIs

```rust
// WRONG: Mixed APIs
use keys::{format_trade_topic, TICK_TOPIC_SIZE};  // Inconsistent naming

// RIGHT: Use current naming
use keys::{format_tick_topic, TICK_TOPIC_SIZE};
```

## Violation Detection

```bash
# Find raw i64 used as market_key without wrapper
rg "market_key:\s*i64" --type rust services/

# Find string topic formatting (should be binary)
rg "format!\(" --type rust services/ | grep -E "market_key|bar_key"

# Find deprecated trade_topic usage
rg "format_trade_topic|parse_trade_topic|TRADE_TOPIC_SIZE" --type rust

# Find deprecated tensor_topic usage
rg "format_tensor_topic|parse_tensor_topic|TENSOR_TOPIC_SIZE" --type rust
```

## Migration Guide

### From String Topics to Binary

```rust
// Before: String topic (slow, allocates)
let topic = format!("trades:{}", market_key);

// After: Binary topic (fast, zero-alloc)
use keys::{format_tick_topic, TopicBuffer};
let mut buf = TopicBuffer::new();
let topic = format_tick_topic(&mut buf, market_key);
```

### From trade_* to tick_*

```rust
// Before (deprecated)
use keys::{format_trade_topic, TRADE_TOPIC_SIZE};

// After
use keys::{format_tick_topic, TICK_TOPIC_SIZE};
```

### From tensor_* to prediction_*

```rust
// Before (deprecated)
use keys::{format_tensor_topic_binary, TENSOR_TOPIC_SIZE};

// After
use keys::{format_prediction_topic_binary, PREDICTION_TOPIC_SIZE};
```

## Cross-References

- **Used by:** All services for routing and topic handling
- **Related skills:** `types.md` (FlatBuffer parsing), `zmq_transport.md` (messaging)
- **Code location:** `crates/keys/src/`

## Key Files

| File | Purpose |
|------|---------|
| `packing.rs` | Key type definitions (MarketKey, BarKey, etc.) |
| `topic.rs` | ZMQ topic format/parse functions |
| `composite_key.rs` | CompositeKey for HashMap routing |
| `ids.rs` | Type-safe ID wrappers |
| `registry.rs` | KeyRegistry for reverse lookups |
