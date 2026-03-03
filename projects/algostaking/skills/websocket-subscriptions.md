# WebSocket Channel Subscriptions

## Overview

The stream gateway (`services/gateway/stream`) provides WebSocket access to real-time data with selective channel subscriptions. Clients only receive data they've explicitly subscribed to.

**Code:** `services/gateway/stream/src/`

## Connection Flow

```
Client connects → Welcome message → Subscribe to channels → Receive data
                                  → Unsubscribe / close
```

### Connection URL

```
ws://localhost:8081/stream              # New style: no initial subscriptions
ws://localhost:8081/stream?subscribe=ticks,bars  # Legacy: subscribe to all
```

### Welcome Message

```json
{
  "type": "connected",
  "connection_id": 12345,
  "max_channels_per_stream": 1000
}
```

## Channel Types

Channels identify specific data streams. Format varies by stream type:

| Stream | Channel Fields | Example |
|--------|---------------|---------|
| Ticks | `market_key` | `{"market_key": 562958543486976001}` |
| Bars | `market_key`, `bar_key` | `{"market_key": ..., "bar_key": 655361}` |
| Features | `market_key`, `bar_key` | Same as bars |
| Signals | `market_key`, `bar_key`, `feature_id` | `{"market_key": ..., "bar_key": ..., "feature_id": 5}` |

**Code:** `services/gateway/stream/src/channel.rs`

## Client Messages

### Subscribe to Channels

```json
{
  "type": "subscribe",
  "stream": "bars",
  "channels": [
    {"market_key": 562958543486976001, "bar_key": 655361},
    {"market_key": 562958543486976001, "bar_key": 655362}
  ],
  "snapshot": 100  // Optional: request N historical items
}
```

### Unsubscribe from Channels

```json
{
  "type": "unsubscribe",
  "stream": "bars",
  "channels": [
    {"market_key": 562958543486976001, "bar_key": 655361}
  ]
}
```

### Subscribe to All (Wildcard)

```json
{"type": "subscribe_all", "stream": "ticks"}
```

### Unsubscribe from All

```json
{"type": "unsubscribe_all", "stream": "bars"}
```

### Ping

```json
{"type": "ping"}
```

## Server Messages

### Subscription Confirmed

```json
{
  "type": "subscribed",
  "stream": "bars",
  "channels": [{"market_key": ..., "bar_key": ...}],
  "total_channels": 5
}
```

### Snapshot Data

When `snapshot` parameter is provided, binary data is sent first, then:

```json
{
  "type": "snapshot",
  "stream": "bars",
  "channel": {"market_key": ..., "bar_key": ...},
  "count": 100,
  "complete": true
}
```

### Unsubscription Confirmed

```json
{
  "type": "unsubscribed",
  "stream": "bars",
  "channels": [...],
  "remaining_channels": 3
}
```

### Error

```json
{
  "type": "error",
  "message": "Too many subscriptions",
  "code": "too_many_subscriptions"
}
```

Error codes:
- `too_many_subscriptions` - Exceeded max_channels_per_stream
- `invalid_channel` - Malformed channel specification

### Pong

```json
{"type": "pong"}
```

## Binary Data

All market data (ticks, bars, features, signals) is sent as **binary FlatBuffer messages**. Parse with the appropriate FlatBuffer schema.

```typescript
// Frontend example
socket.onmessage = (event) => {
  if (event.data instanceof ArrayBuffer) {
    // Binary data - parse as FlatBuffer
    const bytes = new Uint8Array(event.data);
    const bar = Bar.getRootAsBar(new flatbuffers.ByteBuffer(bytes));
  } else {
    // JSON control message
    const msg = JSON.parse(event.data);
  }
};
```

## Snapshot Support

Clients can request historical data on subscription for instant chart population:

```json
{
  "type": "subscribe",
  "stream": "bars",
  "channels": [{"market_key": ..., "bar_key": ...}],
  "snapshot": 200  // Request up to 200 historical bars
}
```

The server:
1. Sends subscription confirmation
2. Sends N binary FlatBuffer messages (historical data)
3. Sends snapshot metadata message with count
4. Continues streaming live data

**Snapshot cache:** In-memory ring buffers per channel (`broadcast/snapshot.rs`)

## Frontend Integration

The frontend WebSocket store provides React hooks:

```typescript
// Subscribe to specific bar channel
const { bars, latestBar } = useBarChannel(marketKey, barKey);

// Subscribe to signal channel
const { signals, latestSignal } = useSignalChannel(marketKey, barKey, featureId);

// Subscribe to ticks
const { ticks } = useTickChannel(marketKey);
```

**Code:** `algostaking-app/src/store/websocket.ts`

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   Stream Gateway (8081)                  │
├─────────────────────────────────────────────────────────┤
│  ConnectionRegistry (manages all WebSocket connections) │
│    ├─ Tick subscriptions (per-market refcount)         │
│    ├─ Bar subscriptions (per-market+bar refcount)      │
│    ├─ Feature subscriptions                            │
│    └─ Signal subscriptions (per-market+bar+feature)    │
├─────────────────────────────────────────────────────────┤
│  SnapshotCache (in-memory ring buffers per channel)    │
│    └─ Stores last N items for instant chart population │
├─────────────────────────────────────────────────────────┤
│  ZMQ Subscribers (one per stream type)                 │
│    └─ tick_sub(5555), bar_sub(5556), feature_sub(5557) │
└─────────────────────────────────────────────────────────┘
```

## Performance Notes

- **Batching:** Frontend batches subscribe/unsubscribe requests (20ms debounce)
- **Refcounting:** Multiple clients subscribing to same channel share one subscription
- **Binary topics:** ZMQ uses binary topic matching (no string parsing)
- **Zero-copy:** FlatBuffer data passed directly from ZMQ to WebSocket

## Limits

| Limit | Default | Config |
|-------|---------|--------|
| Max connections | 10,000 | `config.max_connections` |
| Max channels per stream | 1,000 | `config.max_channels_per_stream` |
| Channel buffer size | 1,000 | `config.channel_buffer_size` |
| Connection timeout | 300s | `config.connection_timeout_secs` |
