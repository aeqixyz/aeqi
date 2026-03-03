# Pipeline: Trading

## Services

| Service | Port | Purpose |
|---------|------|---------|
| **pms** | 9012 | Portfolio Management System - Signal → TargetPosition with Kelly sizing |
| **oms** | 9013 | Order Management System - TargetPosition → Order with fill tracking |
| **ems** | 9009 | Execution Management System - Multi-account order execution |

## Data Flow

```
From Signal Pipeline (5561)
          │
          │ Signal FlatBuffer
          │ Topic: [market_key:8][bar_key:4][feature_id:2]
          ▼
┌───────────────────┐
│       PMS         │
│  (Port 9012)      │
│                   │
│  • Kelly sizing   │
│  • Risk limits    │
│  • Exposure mgmt  │
│  • Account select │
└─────────┬─────────┘
          │ ZMQ PUB 5570
          │ Binary: OpenTrade (128 bytes)
          ▼
┌───────────────────┐
│       OMS         │
│  (Port 9013)      │
│                   │
│  • Order create   │
│  • State machine  │
│  • Fill tracking  │
│  • Deduplication  │
└─────────┬─────────┘
          │ ZMQ PUB 5564 (Orders)
          │ ZMQ SUB 5565 (Fills)
          │ ZMQ SUB 5568 (Acks)
          ▼
┌───────────────────┐
│       EMS         │
│  (Port 9009)      │
│                   │
│  • Venue connect  │
│  • Smart routing  │
│  • Paper/Live     │
│  • Fill reporting │
└───────────────────┘
          │
          ▼
    Exchanges / Paper Sim
```

## ZMQ Topics

| Direction | Port | Format | Payload |
|-----------|------|--------|---------|
| Signal → PMS | 5561 | Binary 14 bytes | Signal FlatBuffer |
| PMS → OMS | 5570 | Binary 8 bytes (market_key) | OpenTrade (128 bytes fixed) |
| OMS → EMS | 5564 | String topic | OrderEvent FlatBuffer |
| EMS → OMS | 5565 | String topic | FillEvent FlatBuffer |
| EMS → OMS | 5568 | String topic | OrderAck FlatBuffer |
| OMS → PMS | 5571 | String topic | PositionUpdate FlatBuffer |
| EMS → PMS | 5566 | String topic | AccountState FlatBuffer |

## Order Flow State Machine

```
┌─────────────┐
│ PendingNew  │  Order created, waiting to submit
└──────┬──────┘
       │ submit()
       ▼
┌─────────────┐
│    New      │  Submitted to EMS, awaiting ack
└──────┬──────┘
       │ ack received
       ▼
┌─────────────┐     fill()      ┌───────────────┐
│ Acknowledged│────────────────▶│PartiallyFilled│
└──────┬──────┘                 └───────┬───────┘
       │ fill()                         │ fill()
       ▼                                ▼
┌─────────────┐               ┌─────────────┐
│   Filled    │ (terminal)    │   Filled    │
└─────────────┘               └─────────────┘

       │ cancel()                   │ cancel()
       ▼                            ▼
┌─────────────┐               ┌─────────────┐
│PendingCancel│───────────────│ Cancelled   │ (terminal)
└─────────────┘               └─────────────┘

       │ reject()
       ▼
┌─────────────┐
│  Rejected   │ (terminal)
└─────────────┘
```

## Latency Targets

| Operation | Target | Typical | Notes |
|-----------|--------|---------|-------|
| PMS sizing | <50μs | 20μs | Kelly calculation |
| OMS order create | <100μs | 50μs | State machine |
| EMS paper fill | <1ms | 500μs | Simulated |
| EMS live submit | <10ms | 5ms | Network dependent |

## Key Patterns

### PMS: Kelly Position Sizing

```rust
impl PortfolioManager {
    fn size_position(&self, signal: &SignalData, account: &Account) -> Option<OpenTrade> {
        // Kelly fraction from signal
        let edge = signal.magnitude_pct.abs();
        let variance = signal.bar_volatility().powi(2);
        let kelly = (edge / variance) * signal.cross_resolution_agreement as f64;

        // Apply risk limits
        let capped_kelly = kelly.min(account.max_kelly_fraction());

        // Convert to notional
        let capital = self.get_available_capital(account);
        let notional = capital * capped_kelly;

        // Check position limits
        if notional < MIN_POSITION_NOTIONAL {
            return None;
        }

        Some(OpenTrade::from_signal(signal, trade_id, account.id, notional, seq))
    }
}
```

### PMS: Risk Checks

```rust
fn passes_risk_checks(&self, account: &Account, signal: &SignalData) -> bool {
    // Drawdown limits
    let current_drawdown = self.calculate_drawdown(account);
    if current_drawdown > account.max_drawdown_pct {
        return false;
    }

    // Daily drawdown
    let daily_drawdown = self.calculate_daily_drawdown(account);
    if daily_drawdown > account.max_daily_drawdown_pct {
        return false;
    }

    // Correlation check (don't stack correlated positions)
    if self.has_correlated_position(account, signal.market_key) {
        return false;
    }

    true
}
```

### OMS: Order State Machine

```rust
use types::{ManagedOrder, ManagedOrderStatus};

impl OrderManager {
    fn handle_fill(&mut self, fill: &FillEvent) -> Result<(), Error> {
        let order = self.orders.get_mut(&fill.order_id)
            .ok_or(Error::UnknownOrder)?;

        // Validate state transition
        if !order.status.can_fill() {
            return Err(Error::InvalidStateTransition);
        }

        // Apply fill
        order.apply_fill(fill.fill_quantity, fill.fill_price, fill.timestamp_us);

        // Emit position update
        self.emit_position_update(&order)?;

        Ok(())
    }
}
```

### OMS: Fill Deduplication

```rust
use lru::LruCache;

struct FillDeduplicator {
    seen: LruCache<u64, ()>,  // fill_id → seen
}

impl FillDeduplicator {
    fn is_duplicate(&mut self, fill_id: u64) -> bool {
        if self.seen.contains(&fill_id) {
            return true;
        }
        self.seen.put(fill_id, ());
        false
    }
}
```

### EMS: Paper vs Live Execution

```rust
impl ExecutionManager {
    async fn execute(&mut self, order: &OrderEvent) -> Result<(), Error> {
        let account = self.accounts.get(&order.account_id)?;

        if account.is_live {
            self.execute_live(order).await
        } else {
            self.execute_paper(order).await
        }
    }

    async fn execute_paper(&mut self, order: &OrderEvent) -> Result<(), Error> {
        // Simulate fill at limit price (or market)
        let fill_price = order.limit_price.unwrap_or(self.get_market_price(order));
        let fill = FillEvent {
            order_id: order.order_id,
            fill_price,
            fill_quantity: order.quantity,
            is_maker: order.limit_price.is_some(),
            // ...
        };
        self.emit_fill(fill).await
    }
}
```

### EMS: Account State Broadcasting

```rust
// Periodic account state updates (1s)
async fn broadcast_account_states(&self) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        for account in self.accounts.values() {
            let state = AccountState {
                account_id: account.id,
                total_equity: self.calculate_equity(account),
                available_balance: self.calculate_available(account),
                positions: self.get_positions(account),
                // ...
            };
            self.emit_account_state(state).await;
        }
    }
}
```

## Binary Message Formats

### OpenTrade (128 bytes, fixed)

```
Offset  Size  Field
0       8     trade_id: u64
8       8     sequence: u64
16      8     account_id: i64
24      8     market_key: i64
32      4     bar_key: i32
36      1     side: TradeSide (0=Long, 1=Short, 2=Flat)
37      3     padding
40      8     target_notional: f64
48      4     signal_quality: f32
52      4     padding
56      8     signal_id: u64
64      8     signal_bar: u64
72      8     signal_magnitude: f64
80      8     signal_retracement: f64
88      4     signal_horizon: u32
92      4     padding
96      8     reference_price: f64
104     8     reference_volatility: f64
112     8     timestamp_us: u64
120     8     reserved
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| Double fills | Missing deduplication | Check LRU cache size |
| Order stuck | State machine bug | Check transition logic |
| Wrong sizing | Stale capital | Check account state sync |
| Paper fills slow | Blocking in EMS | Check async execution |

## Required Crate Skills

Before modifying this pipeline, read:
1. `.claude/skills/crates/types.md` - OpenTrade, ManagedOrder, OrderSide
2. `.claude/skills/crates/keys.md` - Market key for routing
3. `.claude/skills/crates/ports.md` - OMS/EMS port constants

## Service-Specific Skills

- `.claude/skills/services/pms.md`
- `.claude/skills/services/oms.md`
- `.claude/skills/services/ems.md`

## Monitoring

| Service | Key Metrics |
|---------|-------------|
| PMS | `positions_opened_total`, `kelly_fraction_histogram`, `risk_rejections` |
| OMS | `orders_created_total`, `fills_processed_total`, `state_transitions` |
| EMS | `orders_submitted_total`, `fill_latency_ms`, `paper_vs_live` |

## Configuration

### PMS (`config/dev/pms.yaml`)
```yaml
zmq:
  sub_endpoint: "tcp://127.0.0.1:5561"
  pub_endpoint: "tcp://0.0.0.0:5570"
risk:
  max_kelly_fraction: 0.05
  max_position_pct: 0.10
  max_drawdown_pct: 0.10
  max_daily_drawdown_pct: 0.05
```

### OMS (`config/dev/oms.yaml`)
```yaml
zmq:
  sub_targets: "tcp://127.0.0.1:5570"
  sub_fills: "tcp://127.0.0.1:5565"
  pub_orders: "tcp://0.0.0.0:5564"
dedup:
  lru_size: 10000
```

### EMS (`config/dev/ems.yaml`)
```yaml
zmq:
  sub_orders: "tcp://127.0.0.1:5564"
  pub_fills: "tcp://0.0.0.0:5565"
  pub_account_state: "tcp://0.0.0.0:5566"
paper:
  slippage_bps: 5
  fill_probability: 0.95
```
