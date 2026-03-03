# Protocol Compatibility & Binary Serialization

## Golden Rule

**NEVER duplicate binary parsing/serialization logic. ALWAYS use canonical methods from the `types` crate.**

When sending or receiving binary data between services, there is exactly ONE source of truth: the struct definition in `crates/types/src/lib.rs` and its `to_bytes()` / `from_bytes()` methods.

## Why This Matters

Binary protocols are **byte-position sensitive**. A single byte offset error corrupts all downstream fields:

```text
WRONG: Subscriber parses at offset 37, gets garbage
RIGHT: Subscriber uses Type::from_bytes() which knows the exact layout
```

Real failure mode (from production incident):
- PMS publishes `OpenTrade` with 128-byte binary format
- Persistence subscriber had custom parsing that was 13+ bytes out of sync
- Result: `timestamp_us` parsed garbage → "timestamp out of range" errors
- Impact: 76,265+ failed writes, broken audit trail

## Canonical Patterns

### Publishing Data

```rust
// In service that PUBLISHES data
use types::OpenTrade;

let opentrade = OpenTrade {
    trade_id: 123,
    timestamp_us: now_us(),
    // ... all fields
};

// Use canonical serialization
let bytes = opentrade.to_bytes();  // Returns [u8; 128]
socket.send(bytes);
```

### Receiving Data

```rust
// In service that SUBSCRIBES to data
use types::OpenTrade;

let payload = &msg_bytes[TOPIC_SIZE..];

// Use canonical deserialization - NEVER parse bytes manually!
match OpenTrade::from_bytes(payload) {
    Some(opentrade) => {
        // Use opentrade fields directly or convert to local struct
        let local = LocalData::from(opentrade);
    }
    None => {
        warn!("Failed to parse OpenTrade");
    }
}
```

### Converting to Local Types

If you need a local wrapper struct (e.g., for database compatibility):

```rust
// Local struct for DB compatibility
pub struct OpenTradeData {
    pub trade_id: u64,
    pub account_id: i64,  // might need type conversion
    // ... fields matching OpenTrade
}

// Implement From trait - fields map 1:1
impl From<types::OpenTrade> for OpenTradeData {
    fn from(ot: types::OpenTrade) -> Self {
        Self {
            trade_id: ot.trade_id,
            account_id: ot.account_id,
            // ... direct mapping
        }
    }
}
```

## Binary Format Anatomy

Each struct in `crates/types/` has a fixed size with explicit padding:

```rust
pub struct OpenTrade {
    pub trade_id: u64,           // offset 0,  size 8
    pub sequence: u64,           // offset 8,  size 8
    pub account_id: i64,         // offset 16, size 8
    pub market_key: i64,         // offset 24, size 8
    pub bar_key: i32,            // offset 32, size 4
    pub side: TradeSide,         // offset 36, size 1
    _pad1: [u8; 3],              // offset 37, size 3  ← PADDING!
    pub target_notional: f64,    // offset 40, size 8
    pub signal_quality: f32,     // offset 48, size 4
    _pad2: [u8; 4],              // offset 52, size 4  ← PADDING!
    // ... continues to 128 bytes
}
```

The `to_bytes()` and `from_bytes()` methods handle all padding automatically. Manual parsing will **always** drift out of sync when fields change.

## Anti-Patterns to Avoid

### 1. Manual Byte Slicing

```rust
// WRONG - will break when struct changes
let trade_id = u64::from_le_bytes(data[0..8].try_into().unwrap());
let sequence = u64::from_le_bytes(data[8..16].try_into().unwrap());
let side = data[36];  // Forgot about padding after bar_key!

// RIGHT - use canonical method
let ot = OpenTrade::from_bytes(data).expect("valid format");
```

### 2. Duplicating Struct Definitions

```rust
// WRONG - separate struct with same layout
pub struct MyOpenTrade {
    pub trade_id: u64,
    // ... copying fields
}

impl MyOpenTrade {
    fn from_bytes(data: &[u8]) -> Self {
        // DUPLICATED PARSING - will drift!
    }
}

// RIGHT - use types crate directly or impl From
use types::OpenTrade;
let ot = OpenTrade::from_bytes(data)?;
```

### 3. Assuming Field Order

```rust
// WRONG - assumes fields are contiguous without padding
for i in 0..8 { trade_id_bytes[i] = data[i]; }
for i in 0..8 { sequence_bytes[i] = data[8+i]; }
// What about padding? Alignment? You don't know!

// RIGHT - let the canonical method handle it
let ot = OpenTrade::from_bytes(data)?;
```

## Message Format Reference

| Message Type | Size | Canonical Type | Publisher | Subscribers |
|--------------|------|----------------|-----------|-------------|
| Tick | 56 bytes | `types::NormalizedTick` | ingestion | aggregation, persistence |
| Bar | 144 bytes | `types::Bar` | aggregation | feature, persistence, stream |
| Feature | 64 bytes | `types::Feature` | feature | prediction, persistence |
| Prediction | 48 bytes | `types::Prediction` | prediction | signal, persistence |
| Signal | 96 bytes | `types::Signal` | signal | pms, stream, persistence |
| OpenTrade | 128 bytes | `types::OpenTrade` | pms | oms, persistence |
| Order | varies | FlatBuffers | oms | ems, persistence |
| Fill | varies | FlatBuffers | ems | oms, persistence |

## Database Schema Sync

When updating binary formats:

1. **Update types crate first** - `crates/types/src/lib.rs`
2. **Update to_bytes()/from_bytes()** - maintain backwards compatibility if needed
3. **Create migration** - `infrastructure/migrations/XXX_description.sql`
4. **Update schema files** - `infrastructure/schema/*.sql` (source of truth for fresh installs)
5. **Update all subscribers** - they should use `Type::from_bytes()` so just rebuild

## Testing Protocol Compatibility

```rust
#[test]
fn test_opentrade_roundtrip() {
    let original = OpenTrade {
        trade_id: 123,
        // ... all fields
    };

    let bytes = original.to_bytes();
    assert_eq!(bytes.len(), 128);  // Size is part of protocol

    let parsed = OpenTrade::from_bytes(&bytes).unwrap();
    assert_eq!(original.trade_id, parsed.trade_id);
    // ... verify all fields
}
```

## Debugging Format Issues

If you see timestamp/numeric field errors:

1. **Check byte alignment** - Print `payload.len()` vs expected size
2. **Verify topic stripping** - Topic bytes must be removed before parsing
3. **Hex dump comparison** - Compare publisher output with subscriber input
4. **Use canonical parsing** - If custom parsing exists, replace with `Type::from_bytes()`

```bash
# Find where topic is stripped
rg "payload|topic" services/data/persistence/src/transport/

# Verify struct sizes match
rg "const.*SIZE|from_bytes|to_bytes" crates/types/src/lib.rs
```

## Code Location

- **Canonical types:** `crates/types/src/lib.rs`
- **Key packing:** `crates/keys/src/packing.rs`
- **Topic handling:** `crates/keys/src/topic.rs`
- **ZMQ transport:** `crates/zmq_transport/src/`
