# Pipeline Semantics

## Data Flow Overview

```
Exchange WS → Ingestion → Aggregation → Feature → Prediction → Signal → PMS → OMS → EMS
     ↓            ↓            ↓           ↓           ↓          ↓       ↓       ↓
   Raw         Tick       AVBB Bar     Feature    Prediction  Signal   Position  Order
   JSON     FlatBuffer   FlatBuffer    Vector      Tensor     Score    Request  Execution
```

## Key Types Reference

All keys use packed integer representation. See `keys-packing.md` for full details.

| Key | Type | Size | Composition |
|-----|------|------|-------------|
| `market_key` | `i64` | 8 bytes | inst_type\|base\|quote\|venue (4×u16) |
| `bar_key` | `i32` | 4 bytes | bar_type\|variant (2×u16) |
| `feature_id` | `u16` | 2 bytes | Feature schema ID (alias: `schema_id`) |
| `StrategyKey` | composite | 14 bytes | market_key + bar_key + feature_id |

## Stage Details

### 1. Data Ingestion (Port 9000)

**Input:** Raw WebSocket JSON from exchanges (Binance, Bybit, Hyperliquid, etc.)

**Processing:**
- Parse JSON with simdjson (zero-copy, SIMD-accelerated)
- Normalize to internal Tick format
- Build FlatBuffer message (zero-allocation)
- Publish to ZMQ PUB socket with binary topic (8 bytes)

**Output:** Normalized Tick FlatBuffer
```
Tick {
    market_key: i64,      // Packed: [inst_type:16|base:16|quote:16|venue:16]
    price: f64,
    quantity: f64,
    side: Side,           // Buy/Sell
    timestamp_us: u64,    // Microseconds since epoch
}
```

**ZMQ Topic:** 8-byte binary `market_key` (native endian)

**Latency Target:** <2.5us parse + <500ns publish

### 2. Data Aggregation (Port 9001)

**Input:** Trade FlatBuffers from ingestion

**Processing:**
- Subscribe to trade stream via ZMQ SUB
- Update market state for each symbol
- Emit bars when volatility/time/tick thresholds hit
- Schedule time-based bar closures

**Bar Types:**

| Type | Trigger | Use Case |
|------|---------|----------|
| PRBB | Parkinson range threshold | Volatility clustering |
| PKBB | Parkinson-Kunitomo threshold | Drift-adjusted volatility |
| RSBB | Rogers-Satchell threshold | Mean-reversion detection |
| GKBB | Garman-Klass threshold | Efficiency-weighted volatility |
| VWBB | VWAP deviation threshold | Volume-weighted moves |
| RVBB | Realized variance threshold | Statistical volatility |
| Tick | N trades | Fixed trade count |
| Time | T seconds | Fixed time interval |

**Output:** Bar FlatBuffer
```
Bar {
    market_key: i64,      // 8-byte packed key
    bar_key: i32,         // 4-byte packed: [bar_type:16|variant:16]
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    vwap: f64,
    trade_count: u32,
    open_time_us: u64,
    close_time_us: u64,
}
```

**ZMQ Topic:** 12-byte binary `[market_key:8][bar_key:4]` (native endian)

**Latency Target:** <5us per trade processing

### 3. Feature Engineering (Port 9003)

**Input:** Bar FlatBuffers from aggregation

**Processing:**
- DAG-based feature computation
- Lookback window management (1min, 5min, 15min, 1hr, 4hr, 1d)
- Feature normalization (z-score, rank, percentile)
- SIMD-vectorized calculations

**Feature Categories:**

| Category | Examples | Lookbacks |
|----------|----------|-----------|
| Momentum | ROC, RSI, MACD | 5m, 15m, 1h |
| Mean Reversion | Bollinger %B, Z-score | 1h, 4h, 1d |
| Microstructure | Spread, Depth Imbalance | 1m, 5m |
| Volume | OBV, Volume Ratio | 15m, 1h |
| Volatility | ATR, Parkinson | 1h, 4h, 1d |
| Cross-Asset | Correlation, Beta | 1d, 7d |

**Output:** Feature Vector (normalized tensor)
```
Features {
    market_key: i64,      // 8-byte packed key
    bar_key: i32,         // 4-byte packed key
    feature_id: u16,      // Feature schema identifier (alias: schema_id)
    timestamp_us: u64,
    data: [f32; N],       // Schema-specific feature count
}
```

**ZMQ Topic:** 14-byte binary `[market_key:8][bar_key:4][feature_id:2]` (native endian)

**Latency Target:** <20us per bar

### 4. Prediction (Port 9004)

**Input:** Feature vectors from feature engineering

**Processing:**
- Fourier Neural Operator (FNO) inference
- 3D tensor construction (time x frequency x features)
- Multi-horizon forecasting
- GPU or optimized CPU inference

**Model Architecture:**
```
Input: [batch, time, features] → FNO Layers → Output: [batch, horizons, predictions]
                                    ↓
                              Spectral Convolution
                              (Efficient long-range dependencies)
```

**Output:** Prediction Tensor
```
Prediction {
    market_key: i64,      // 8-byte packed key
    bar_key: i32,         // 4-byte packed key
    feature_id: u16,      // Feature schema identifier
    timestamp_us: u64,
    data: Tensor,         // FNO output tensor
}
```

**ZMQ Topic:** 14-byte binary `[market_key:8][bar_key:4][feature_id:2]` (native endian)

**Latency Target:** <100us inference

### 5. Signal Generation (Port 9008)

**Input:** Predictions from inference

**Processing:**
- LTC (Liquid Time-Constant) network aggregation
- Multi-horizon signal fusion
- Risk-adjusted expected return calculation
- Position sizing via Kelly criterion

**Signal Calculation:**
```
signal = Σ(prediction[h] × confidence[h] × horizon_weight[h])
kelly_fraction = (edge / variance) × risk_multiplier
position_size = capital × kelly_fraction × leverage_limit
```

**Output:** Trading Signal
```
Signal {
    signal_id: u64,        // Unique signal identifier
    market_key: i64,       // 8-byte packed key
    bar_key: i32,          // 4-byte packed key
    feature_id: u16,       // Feature schema (alias: schema_id)
    signal_bar: u64,       // Bar number that triggered signal
    timestamp_us: u64,
    magnitude_pct: f64,    // Expected move percentage
    retracement_pct: f64,  // Expected retracement
    cross_resolution_agreement: f32,  // Multi-timeframe agreement
    resolution_count: u8,  // Number of resolutions agreeing
    inference_latency_ns: u64,  // Processing time
}
```

**ZMQ Topic:** 14-byte binary `[market_key:8][bar_key:4][feature_id:2]` (native endian)

**Latency Target:** <10us

### 6. Portfolio Management (Port 9012)

**Input:** Signals from signal generation

**Processing:**
- Portfolio-level risk aggregation
- Position limit enforcement
- Correlation-based exposure management
- P&L tracking and drawdown monitoring

**Risk Limits:**
- Max position per symbol: 5% of capital
- Max sector exposure: 20% of capital
- Max total exposure: 100% (no leverage) or configured limit
- Daily drawdown limit: 2% of capital

**Output:** Position Request
```
PositionRequest {
    market_key: u64,
    timestamp: u64,
    target_position: f64,  // Target notional
    urgency: Urgency,      // Low/Medium/High
    reason: String,        // Signal ID for audit
}
```

### 7. Order Management (Port 9013)

**Input:** Position requests from PMS

**Processing:**
- Order lifecycle management
- Partial fill handling
- Order amendment/cancellation
- Execution quality monitoring

**Order States:**
```
Pending → Submitted → Acknowledged → PartiallyFilled → Filled
                                   ↓
                              Cancelled / Rejected
```

**Output:** Order to EMS
```
Order {
    order_id: u64,
    market_key: u64,
    side: Side,
    order_type: OrderType,
    price: f64,
    quantity: f64,
    time_in_force: TimeInForce,
}
```

### 8. Execution Management (Port 9009)

**Input:** Orders from OMS

**Processing:**
- Venue selection (best price, lowest latency)
- Smart order routing
- TWAP/VWAP execution algorithms
- FIX/WebSocket venue connectivity

**Execution Algorithms:**
- **Market**: Immediate execution at best available
- **Limit**: Price-protected execution
- **TWAP**: Time-weighted average price
- **VWAP**: Volume-weighted average price
- **Iceberg**: Hidden quantity execution

## ZMQ Topics (Binary Format)

Hot-path topics use **binary** format for zero-allocation. See `keys-packing.md` for details.

| Stage | Topic Size | Format | Port |
|-------|------------|--------|------|
| Ingestion → Tick | 8 bytes | `[market_key:8]` | 5555 |
| Aggregation → Bar | 12 bytes | `[market_key:8][bar_key:4]` | 5556 |
| Feature → Features | 14 bytes | `[market_key:8][bar_key:4][feature_id:2]` | 5557 |
| Prediction → Tensor | 14 bytes | `[market_key:8][bar_key:4][feature_id:2]` | 5558 |
| Signal → Signal | 14 bytes | `[market_key:8][bar_key:4][feature_id:2]` | 5561 |

**Note:** All topics use native-endian byte order (little-endian on x86_64).

## Checkpointing

Services persist state for recovery:

| Service | Checkpoint Data | Frequency |
|---------|-----------------|-----------|
| Aggregation | Bar state per symbol | On bar close |
| Feature | Lookback window data | Every 100 bars |
| Prediction | Encoder hidden state | Every 1000 inferences |
| Signal | LTC network state | Every 1000 signals |

## Monitoring

Critical metrics per stage:

| Stage | Key Metrics |
|-------|-------------|
| Ingestion | Trades/sec, Parse latency P99, Reconnects |
| Aggregation | Bars/sec, Process latency P99, Active symbols |
| Feature | Features/sec, Calc latency P99 |
| Prediction | Inferences/sec, Inference latency P99 |
| Signal | Signals/sec, Signal latency P99 |
| OMS | Orders/sec, Fill rate, Slippage |
