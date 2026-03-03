# Service: configuration

## Required Reading
1. `.claude/skills/pipelines/gateway.md`
2. `.claude/skills/crates/types.md` - Registry FlatBuffer types

## Purpose

Service configuration distribution via ZMQ REQ/REP and PUB patterns.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point |
| `src/registry.rs` | Asset, venue, bar type registry |
| `src/subscription.rs` | Subscription management |
| `src/cache.rs` | In-memory caching |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `database.url` | env | PostgreSQL connection |
| `cache.ttl_seconds` | `300` | Cache TTL |
| `zmq.rep_endpoint` | `tcp://0.0.0.0:5560` | REQ/REP queries |
| `zmq.pub_endpoint` | `tcp://0.0.0.0:5560` | Subscription updates |

## ZMQ Connections

| Direction | Port | Topic | Description |
|-----------|------|-------|-------------|
| IN/OUT | 5560 REQ/REP | - | Registry queries |
| OUT | 5560 PUB | String prefix | Subscription updates |

## Registry Types

| Type | Description |
|------|-------------|
| Asset | Base/quote assets (BTC, ETH, USDT) |
| Venue | Exchange configurations |
| VenueFee | Maker/taker fees |
| InstrumentType | Spot, perp-linear, perp-inverse |
| BarType | AVBB, time, tick bar types |
| Feature | Feature schema definitions |

## Subscription Topics

```rust
SUBSCRIPTION_MARKET_ADD     // "sub.market.add"
SUBSCRIPTION_MARKET_REMOVE  // "sub.market.remove"
SUBSCRIPTION_BAR_ADD        // "sub.bar.add"
SUBSCRIPTION_BAR_REMOVE     // "sub.bar.remove"
SUBSCRIPTION_FEATURE_ADD    // "sub.feature.add"
SUBSCRIPTION_FEATURE_REMOVE // "sub.feature.remove"
```

## Testing

```bash
cargo build --release -p configuration
cargo test --release -p configuration
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| Stale data | Cache not invalidated | Check PUB subscription |
| Slow queries | Cache miss | Check cache TTL |
