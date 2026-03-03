# Key Packing & Topic System

## Overview

AlgoStaking uses packed integer keys for zero-allocation routing. All keys are designed for:
- Cache-friendly contiguous storage
- Fast hash operations
- Zero-copy ZMQ topic matching
- Type-safe component access

## Key Types

### MarketKey (i64, 8 bytes)

Identifies a tradeable instrument. Packed from 4 u16 components:

```text
bits [63:48] = instrument_type_id (u16)  # spot, perp-linear, perp-inverse, etc.
bits [47:32] = base_asset_id (u16)       # BTC=100, ETH=101, etc.
bits [31:16] = quote_asset_id (u16)      # USDT=200, USD=201, etc.
bits [15:0]  = venue_id (u16)            # Binance=1, Bybit=2, etc.
```

**Example:** BTC-USDT perp on Binance
```rust
let market = MarketKey::new(2, 100, 200, 1);
// = (2 << 48) | (100 << 32) | (200 << 16) | 1
// = 562958543486976001_i64
```

**Code:** `crates/keys/src/packing.rs:12-98`

### BarKey (i32, 4 bytes)

Identifies a bar aggregation type and variant. Packed from 2 u16 components:

```text
bits [31:16] = bar_type_id (u16)   # PRBB=1, PKBB=2, Time=10, etc.
bits [15:0]  = variant_id (u16)    # 1m=1, 5m=2, 1h=3, etc.
```

**Example:** 1-minute time bar
```rust
let bar = BarKey::new(10, 1);  // bar_type=10 (time), variant=1 (1m)
// = (10 << 16) | 1 = 655361_i32
```

**Code:** `crates/keys/src/packing.rs:100-169`

### StrategyKey (14 bytes)

The complete identifier for a trading strategy. Combines market, bar, and feature:

```text
bytes [0:7]   = market_key (i64, 8 bytes)
bytes [8:11]  = bar_key (i32, 4 bytes)
bytes [12:13] = feature_id (u16, 2 bytes)
```

**Total: 14 bytes = matches ZMQ topic size exactly**

```rust
let strategy = StrategyKey::new(market, bar, 5);  // feature_id = 5
strategy.to_topic_bytes()  // Returns [u8; 14] for ZMQ
```

**Key methods:**
- `market_key()` / `bar_key()` / `feature_id()` - component access
- `venue_id()` / `base_asset_id()` - deep component access
- `to_topic_bytes()` / `from_topic_bytes()` - ZMQ format
- `fno_head_key()` / `ltc_head_key()` - derived routing keys

**Code:** `crates/keys/src/packing.rs:462-710`

### ModelKey (u64)

Identifies neural network model weights (bar_type + feature_id):

```text
bits [63:32] = bar_type_id (u16)
bits [31:0]  = feature_id (u16)
```

Used by: feature service, prediction service, signal service

**Code:** `crates/keys/src/packing.rs:189-281`

### FnoHeadKey (u64)

Venue-agnostic prediction head (for FNO inference):

```text
bits [31:16] = bar_type_id (u16)
bits [15:0]  = base_asset_id (u16)
```

Shares heads across venues (BTC head works for Binance, Bybit, etc.)

**Code:** `crates/keys/src/packing.rs:308-369`

### LtcHeadKey (16 bytes)

Venue-specific signal head (for LTC network):

```text
model_key: ModelKey (8 bytes)
symbol_key: i64 (8 bytes, full MarketKey)
```

Maintains venue-specific hidden state trajectories.

**Code:** `crates/keys/src/packing.rs:371-456`

## Binary ZMQ Topics

All hot-path topics use **binary format** (native endian) for zero-allocation:

### Topic Sizes

| Topic Type | Size | Format |
|------------|------|--------|
| Tick | 8 bytes | `[market_key: 8]` |
| Bar | 12 bytes | `[market_key: 8][bar_key: 4]` |
| Feature | 14 bytes | `[market_key: 8][bar_key: 4][feature_id: 2]` |
| Prediction | 14 bytes | `[market_key: 8][bar_key: 4][feature_id: 2]` |
| Signal | 14 bytes | `[market_key: 8][bar_key: 4][feature_id: 2]` |

### Constants

```rust
pub const TICK_TOPIC_SIZE: usize = 8;
pub const BAR_TOPIC_SIZE: usize = 12;
pub const FEATURE_TOPIC_SIZE: usize = 14;
pub const PREDICTION_TOPIC_SIZE: usize = 14;
```

**Code:** `crates/keys/src/topic.rs:162-185`

### Topic Functions

```rust
// Tick topics (8 bytes)
let topic = format_tick_topic(market_key);
let market = parse_tick_topic(&topic);

// Bar topics (12 bytes)
let topic = format_bar_topic_binary(market_key, bar_key);
let (market, bar) = parse_bar_topic_binary(&topic);

// Feature/Prediction topics (14 bytes)
let topic = format_feature_topic_binary(market_key, bar_key, feature_id);
let (market, bar, feature) = parse_feature_topic_binary(&topic);
```

**Code:** `crates/keys/src/topic.rs:231-416`

## Naming Conventions

| Term | Alias | Used In |
|------|-------|---------|
| `feature_id` | `schema_id` | Strategy context, WebSocket |
| `market_key` | `symbol_key` | Runtime routing |
| `bar_key` | - | Bar aggregation |

**Note:** `schema_id` and `feature_id` are the same value. Frontend uses `feature_id`, some backend code uses `schema_id` for historical reasons.

## WebSocket Channel Subscriptions

The WebSocket gateway uses a simplified `Channel` struct:

```rust
pub struct Channel {
    pub market_key: i64,           // Required
    pub bar_key: Option<i32>,      // For bars/features/signals
    pub feature_id: Option<u16>,   // For signals only (alias: schema_id)
}
```

**Channel hierarchy:**
- **Tick channel:** `market_key` only
- **Bar channel:** `market_key + bar_key`
- **Feature channel:** `market_key + bar_key`
- **Signal channel:** `market_key + bar_key + feature_id`

**Code:** `services/gateway/stream/src/channel.rs`

## Performance

All hot-path functions are `#[inline(always)]` with zero allocations:
- Key packing/unpacking: ~1ns
- Topic formatting: ~2ns
- Topic parsing: ~3ns
- HashMap lookup with CompositeKey: ~5ns

## Crate Location

All key types and topic functions are in `crates/keys/`:
- `src/lib.rs` - public API and documentation
- `src/packing.rs` - key type definitions
- `src/topic.rs` - ZMQ topic functions
- `src/composite_key.rs` - CompositeKey (market + bar)
- `src/registry.rs` - KeyRegistry for reverse lookups
