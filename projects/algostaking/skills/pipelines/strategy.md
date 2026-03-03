# Pipeline: Strategy

## Services

| Service | Port | Purpose |
|---------|------|---------|
| **feature** | 9003 | DAG-based feature engineering on bars |
| **prediction** | 9004 | Fourier Neural Operator (FNO) inference |
| **signal** | 9008 | Liquid Time-Constant (LTC) network signal aggregation |

## Data Flow

```
From Aggregation (5556)
          │
          │ Bar FlatBuffer
          │ Topic: [market_key:8][bar_key:4]
          ▼
┌───────────────────┐
│     FEATURE       │
│  (Port 9003)      │
│                   │
│  • DAG features   │
│  • Lookback mgmt  │
│  • Normalization  │
└─────────┬─────────┘
          │ ZMQ PUB 5557
          │ Topic: [market_key:8][bar_key:4][feature_id:2]
          │ Payload: FeatureVector FlatBuffer
          ▼
┌───────────────────┐
│    PREDICTION     │
│  (Port 9004)      │
│                   │
│  • FNO inference  │
│  • 3D tensor      │
│  • Multi-horizon  │
│  • Hot-swap model │
└─────────┬─────────┘
          │ ZMQ PUB 5558
          │ Topic: [market_key:8][bar_key:4][feature_id:2]
          │ Payload: Prediction FlatBuffer
          ▼
┌───────────────────┐
│      SIGNAL       │
│  (Port 9008)      │
│                   │
│  • LTC network    │
│  • Multi-res      │
│    fusion         │
│  • Kelly sizing   │
└─────────┬─────────┘
          │ ZMQ PUB 5561
          │ Topic: [market_key:8][bar_key:4][feature_id:2]
          │ Payload: Signal FlatBuffer
          ▼
     To Trading Pipeline
```

## ZMQ Topics

| Direction | Port | Topic Format | Size | Payload |
|-----------|------|--------------|------|---------|
| Aggregation → Feature | 5556 | `[market_key:8][bar_key:4]` | 12 | Bar |
| Feature → Prediction | 5557 | `[market_key:8][bar_key:4][feature_id:2]` | 14 | FeatureVector |
| Prediction → Signal | 5558 | `[market_key:8][bar_key:4][feature_id:2]` | 14 | Prediction |
| Signal → PMS | 5561 | `[market_key:8][bar_key:4][feature_id:2]` | 14 | Signal |
| Prediction → (checkpoints) | 5559 | String prefix | - | Checkpoint |
| Signal → (checkpoints) | 5563 | String prefix | - | Checkpoint |

## Latency Targets

| Operation | Target | Typical | Notes |
|-----------|--------|---------|-------|
| Feature computation | <20μs | 10-15μs | Per bar, SIMD vectorized |
| FNO inference | <100μs | 50-80μs | GPU or optimized CPU |
| LTC aggregation | <10μs | 5μs | Per prediction |
| Total signal | <150μs | 80μs | Bar → Signal |

## Key Patterns

### Feature: DAG-Based Computation

```rust
// Features computed in dependency order
struct FeatureDAG {
    nodes: Vec<FeatureNode>,
    order: Vec<usize>,  // Topological order
}

impl FeatureDAG {
    fn compute(&self, bar: &BarData, lookbacks: &LookbackWindows) -> Vec<f64> {
        let mut values = vec![0.0; self.nodes.len()];

        for &idx in &self.order {
            values[idx] = self.nodes[idx].compute(&values, bar, lookbacks);
        }

        values
    }
}
```

### Feature: Lookback Window Management

```rust
// Ring buffer for efficient lookback
struct LookbackWindows {
    // Per (market, bar_type, lookback_period)
    windows: HashMap<LookbackKey, RingBuffer<BarData>>,
}

// Lookback periods
const LOOKBACKS: &[Duration] = &[
    Duration::from_secs(60),      // 1m
    Duration::from_secs(300),     // 5m
    Duration::from_secs(900),     // 15m
    Duration::from_secs(3600),    // 1h
    Duration::from_secs(14400),   // 4h
    Duration::from_secs(86400),   // 1d
];
```

### Prediction: FNO Architecture

```rust
// Fourier Neural Operator for efficient long-range dependencies
struct FNO {
    encoder: Encoder,           // Input projection
    spectral_conv: Vec<SpectralConv>,  // Fourier space convolutions
    decoder: Decoder,           // Output projection
}

impl FNO {
    fn forward(&self, features: &Tensor) -> Prediction {
        // Input: [batch, time, features]
        let encoded = self.encoder.forward(features);

        // Spectral convolutions (efficient in frequency domain)
        let mut x = encoded;
        for layer in &self.spectral_conv {
            x = layer.forward(&x);
        }

        // Output: [batch, horizons, predictions]
        self.decoder.forward(&x)
    }
}
```

### Prediction: Atomic Model Hot-Swap

```rust
use arc_swap::ArcSwap;

struct PredictionService {
    model: ArcSwap<FNO>,
}

impl PredictionService {
    // Hot-swap without blocking inference
    fn swap_model(&self, new_model: Arc<FNO>) {
        self.model.store(new_model);
    }

    // Inference uses current model
    fn predict(&self, features: &Tensor) -> Prediction {
        let model = self.model.load();  // Arc clone, fast
        model.forward(features)
    }
}
```

### Signal: LTC Network Aggregation

```rust
// Liquid Time-Constant network for continuous-time dynamics
struct LTCNetwork {
    // Venue-specific hidden states
    heads: HashMap<LtcHeadKey, HiddenState>,
}

impl LTCNetwork {
    fn process(&mut self, prediction: &PredictionData) -> SignalData {
        let head_key = LtcHeadKey::from_prediction(prediction);

        // Get or create venue-specific state
        let state = self.heads.entry(head_key)
            .or_insert_with(HiddenState::default);

        // Update state with new prediction
        let (refined_magnitude, refined_retracement) = state.update(
            prediction.magnitude,
            prediction.retracement,
            prediction.horizon,
        );

        SignalData {
            magnitude_pct: refined_magnitude,
            retracement_pct: refined_retracement,
            cross_resolution_agreement: state.agreement(),
            // ...
        }
    }
}
```

### Signal: Kelly Criterion Sizing

```rust
// Position sizing from signal
fn kelly_fraction(signal: &SignalData) -> f64 {
    let edge = signal.magnitude_pct.abs();
    let variance = signal.prediction.bar.volatility.powi(2);
    let quality = signal.cross_resolution_agreement as f64;

    // Kelly formula with quality adjustment
    (edge / variance) * quality * RISK_MULTIPLIER
}
```

## Head Key Types

| Type | Service | Scope | Purpose |
|------|---------|-------|---------|
| `FnoHeadKey` | Prediction | Venue-agnostic | Share heads across venues (BTC works on Binance, Bybit) |
| `LtcHeadKey` | Signal | Venue-specific | Separate hidden state per venue |

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| Feature NaN | Insufficient warmup | Check `warmup_bars_remaining` |
| Prediction spike | Model drift | Check checkpoint age |
| Signal flip-flop | Low agreement | Check `cross_resolution_agreement` |
| High inference latency | GPU contention | Check batch size, CPU fallback |

## Required Crate Skills

Before modifying this pipeline, read:
1. `.claude/skills/crates/types.md` - FeatureData, PredictionData, SignalData
2. `.claude/skills/crates/keys.md` - FnoHeadKey, LtcHeadKey
3. `.claude/skills/crates/zmq_transport.md` - Topic subscriptions

## Service-Specific Skills

- `.claude/skills/services/feature.md`
- `.claude/skills/services/prediction.md`
- `.claude/skills/services/signal.md`

## Monitoring

| Service | Key Metrics |
|---------|-------------|
| Feature | `features_computed_total`, `warmup_remaining`, `dag_latency_ns` |
| Prediction | `predictions_total`, `inference_latency_ns`, `model_version` |
| Signal | `signals_emitted_total`, `agreement_histogram`, `ltc_latency_ns` |

## Configuration

### Feature (`config/dev/feature.yaml`)
```yaml
zmq:
  sub_endpoint: "tcp://127.0.0.1:5556"
  pub_endpoint: "tcp://0.0.0.0:5557"
warmup:
  min_bars: 100
lookbacks:
  - 60s
  - 5m
  - 15m
  - 1h
```

### Prediction (`config/dev/prediction.yaml`)
```yaml
zmq:
  sub_endpoint: "tcp://127.0.0.1:5557"
  pub_endpoint: "tcp://0.0.0.0:5558"
model:
  checkpoint_dir: "/var/lib/algostaking/checkpoints"
  batch_size: 32
inference:
  use_gpu: false
  threads: 4
```

### Signal (`config/dev/signal.yaml`)
```yaml
zmq:
  sub_endpoint: "tcp://127.0.0.1:5558"
  pub_endpoint: "tcp://0.0.0.0:5561"
ltc:
  time_constant: 0.1
  decay_rate: 0.99
kelly:
  risk_multiplier: 0.25
  max_fraction: 0.05
```
