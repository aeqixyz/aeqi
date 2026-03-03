# Crate: ports

## Purpose

**SINGLE SOURCE OF TRUTH** for all network ports in the system. Prevents port conflicts at compile time through const definitions and validation arrays.

## Public API

### ZMQ Data Layer (5553-5557)

| Constant | Port | Purpose |
|----------|------|---------|
| `INGESTION_BACKFILL_*` | 5553 | Historical data REP |
| `INGESTION_PERP_MARKET_*` | 5554 | Perp market metadata PUB |
| `INGESTION_TRADES_*` | 5555 | Raw trades PUB |
| `AGGREGATION_BARS_*` | 5556 | Bars PUB |
| `FEATURE_VECTORS_*` | 5557 | Feature vectors PUB |

### ZMQ Strategy Layer (5558-5563)

| Constant | Port | Purpose |
|----------|------|---------|
| `PREDICTION_*` | 5558 | Predictions PUB |
| `PREDICTION_CHECKPOINTS_*` | 5559 | Model checkpoints PUB |
| `CONFIG_REGISTRY_*` | 5560 | Configuration REP/PUB |
| `SIGNAL_TRADING_*` | 5561 | Trading signals PUB |
| `SIGNAL_CHECKPOINTS_*` | 5563 | LTC checkpoints PUB |

### ZMQ OMS/EMS Layer (5564-5572)

| Constant | Port | Purpose |
|----------|------|---------|
| `OMS_ORDERS_*` | 5564 | Order events (OMS->EMS) |
| `EMS_FILLS_*` | 5565 | Fill events (EMS->OMS) |
| `EMS_ACCOUNT_STATE_*` | 5566 | Account state periodic |
| `OMS_TRADES_*` | 5567 | Trade records |
| `EMS_ORDER_ACKS_*` | 5568 | Order acknowledgments |
| `PMS_INTENTS_*` | 5570 | Position intents (PMS->OMS) |
| `OMS_POSITIONS_*` | 5571 | Position updates |
| `OMS_AUDIT_*` | 5572 | Audit events |

### Prometheus Metrics (9000-9013)

| Constant | Port | Service |
|----------|------|---------|
| `METRICS_INGESTION` | 9000 | ingestion |
| `METRICS_AGGREGATION` | 9001 | aggregation |
| `METRICS_PERSISTENCE` | 9002 | persistence |
| `METRICS_FEATURE` | 9003 | feature |
| `METRICS_INFERENCE` | 9004 | prediction |
| `METRICS_CONFIGURATION` | 9007 | configuration |
| `METRICS_SIGNAL` | 9008 | signal |
| `METRICS_EMS` | 9009 | ems |
| `METRICS_GATEWAY_API` | 9010 | api |
| `METRICS_GATEWAY_STREAM` | 9011 | stream |
| `METRICS_PMS` | 9012 | pms |
| `METRICS_OMS` | 9013 | oms |

### HTTP/WS Gateway (8081-8082)

| Constant | Port | Purpose |
|----------|------|---------|
| `GATEWAY_API_*` | 8082 | REST API |
| `GATEWAY_STREAM_*` | 8081 | WebSocket |

### Helper Functions

```rust
pub fn all_zmq_ports() -> &'static [u16];
pub fn all_metrics_ports() -> &'static [u16];
pub fn is_zmq_port_allocated(port: u16) -> bool;
pub fn is_metrics_port_allocated(port: u16) -> bool;
pub fn next_available_zmq_port() -> u16;
pub fn next_available_metrics_port() -> u16;
```

## Canonical Usage

### Pattern 1: Use Constants in Config Defaults

```rust
use ports::{PREDICTION_CONNECT, METRICS_INFERENCE};

impl Default for PredictionConfig {
    fn default() -> Self {
        Self {
            sub_endpoint: PREDICTION_CONNECT.to_string(),
            metrics_port: METRICS_INFERENCE,
        }
    }
}
```

### Pattern 2: Use BIND for Publishers, CONNECT for Subscribers

```rust
use ports::{AGGREGATION_BARS_BIND, AGGREGATION_BARS_CONNECT};

// Publisher binds
let publisher = ResilientPublisher::new(AGGREGATION_BARS_BIND, metrics).await?;

// Subscriber connects
let subscriber = ResilientSubscriber::new(AGGREGATION_BARS_CONNECT, &[""], parser, metrics).await?;
```

### Pattern 3: Check Allocation Before Adding New Port

```rust
use ports::{is_zmq_port_allocated, next_available_zmq_port};

// Before adding a new service
let new_port = next_available_zmq_port();
assert!(!is_zmq_port_allocated(new_port));
```

## Anti-Patterns

### DON'T: Hardcode Port Numbers

```rust
// WRONG: Magic numbers
let endpoint = "tcp://127.0.0.1:5555";  // What is 5555?

// RIGHT: Named constants
use ports::INGESTION_TRADES_CONNECT;
let endpoint = INGESTION_TRADES_CONNECT;
```

### DON'T: Invent New Ports Without Registration

```rust
// WRONG: Using unregistered port
const MY_SERVICE_PORT: u16 = 5580;  // Not in ports crate!

// RIGHT: Add to ports crate first
// 1. Add const in crates/ports/src/lib.rs
// 2. Add to ALL_ZMQ_PORTS array
// 3. Run cargo test to validate uniqueness
```

### DON'T: Use Deprecated Port Constants

```rust
// WRONG: Deprecated
use ports::INFERENCE_TENSORS_PORT;  // Use PREDICTION_PORT

// RIGHT: Current naming
use ports::PREDICTION_PORT;
```

## Adding a New Service

1. **Check availability:**
   ```bash
   cd /home/claudedev/algostaking-backend
   cargo test -p ports -- --nocapture
   ```

2. **Add constants to `crates/ports/src/lib.rs`:**
   ```rust
   /// MyService: Description
   pub const MYSERVICE_PORT: u16 = 5573;  // Next available
   pub const MYSERVICE_BIND: &str = "tcp://0.0.0.0:5573";
   pub const MYSERVICE_CONNECT: &str = "tcp://127.0.0.1:5573";
   ```

3. **Add to validation array:**
   ```rust
   const ALL_ZMQ_PORTS: &[u16] = &[
       // ... existing
       MYSERVICE_PORT,  // Add here
   ];
   ```

4. **Run tests:**
   ```bash
   cargo test -p ports
   ```

5. **Update doc comment** at top of lib.rs with new port in ASCII table.

## Violation Detection

```bash
# Find hardcoded port numbers (potential violations)
rg ":\s*\d{4,5}" --type rust services/ | grep -E "tcp://|bind|connect"

# Find deprecated port constants
rg "INFERENCE_TENSORS|INFERENCE_CHECKPOINTS" --type rust

# Find magic numbers that look like ports
rg "\b(55\d\d|80\d\d|90\d\d)\b" --type rust services/ | grep -v "ports::"

# Verify all services use ports crate
for svc in services/*/*; do
    if [ -f "$svc/Cargo.toml" ]; then
        if ! grep -q "ports" "$svc/Cargo.toml"; then
            echo "Missing ports dependency: $svc"
        fi
    fi
done
```

## Migration Guide

### From Hardcoded to Constants

```rust
// Before
let endpoint = "tcp://127.0.0.1:5556";

// After
use ports::AGGREGATION_BARS_CONNECT;
let endpoint = AGGREGATION_BARS_CONNECT;
```

### From Deprecated to Current Names

```rust
// Before
use ports::{INFERENCE_TENSORS_PORT, INFERENCE_TENSORS_BIND};

// After
use ports::{PREDICTION_PORT, PREDICTION_BIND};
```

## Cross-References

- **Used by:** All services for network configuration
- **Related skills:** `zmq_transport.md` (uses these ports)
- **Code location:** `crates/ports/src/lib.rs`

## Port Allocation Map

```
ZMQ PORTS (5550-5599)
├── 5553    Ingestion: Backfill REP
├── 5554    Ingestion: Perp Markets PUB
├── 5555    Ingestion: Raw Trades PUB
├── 5556    Aggregation: Bars PUB
├── 5557    Feature: Vectors PUB
├── 5558    Prediction: Tensors PUB
├── 5559    Prediction: Checkpoints PUB
├── 5560    Configuration: Registry REP/PUB
├── 5561    Signal: Trading PUB
├── 5563    Signal: Checkpoints PUB
├── 5564    OMS: Orders PUB
├── 5565    EMS: Fills PUB
├── 5566    EMS: Account State PUB
├── 5567    OMS: Trade Records PUB
├── 5568    EMS: Order Acks PUB
├── 5570    PMS: Intents PUB
├── 5571    OMS: Positions PUB
├── 5572    OMS: Audit PUB
└── 5573-99 AVAILABLE

METRICS (9000-9099)
├── 9000-9013  Service metrics (see table above)
└── 9014-99    AVAILABLE

GATEWAY (8080-8099)
├── 8081    WebSocket Stream
├── 8082    REST API
└── 8083-99 AVAILABLE
```
