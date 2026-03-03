# Feature Engineering for Quantitative Trading

## Overview

Feature engineering transforms raw market data (bars) into predictive signals (feature vectors). The goal is to extract alpha-predictive information while maintaining low latency and avoiding lookahead bias.

## DAG-Based Feature Computation

Features are computed as a Directed Acyclic Graph (DAG) where:
- Leaf nodes are raw bar values (OHLCV)
- Intermediate nodes are derived features
- Dependencies ensure correct computation order

```
              ┌─────────────────────────────────────────┐
              │              Raw Bar Data               │
              │  (Open, High, Low, Close, Volume)       │
              └──────────────────┬──────────────────────┘
                                 │
        ┌────────────────────────┼────────────────────────┐
        ▼                        ▼                        ▼
   ┌─────────┐              ┌─────────┐              ┌─────────┐
   │ Returns │              │ Ranges  │              │ Volume  │
   └────┬────┘              └────┬────┘              └────┬────┘
        │                        │                        │
   ┌────┴────┐              ┌────┴────┐              ┌────┴────┐
   ▼         ▼              ▼         ▼              ▼         ▼
Momentum  Mean-Rev      Volatility  Trend        OBV    Vol Ratio
  (RSI)   (Z-score)      (ATR)    (ADX)
```

## Feature Categories

### 1. Momentum Features

Capture trending behavior and velocity of price moves.

| Feature | Formula | Lookback | Notes |
|---------|---------|----------|-------|
| ROC | `(close - close[n]) / close[n]` | 5, 15, 60 bars | Rate of change |
| RSI | `100 - 100/(1 + avg_gain/avg_loss)` | 14 bars | Relative strength |
| MACD | `EMA(12) - EMA(26)` | 12, 26, 9 | Trend crossover |
| Momentum | `close - close[n]` | 10 bars | Simple momentum |
| Williams %R | `(high[n] - close) / (high[n] - low[n])` | 14 bars | Overbought/oversold |

### 2. Mean Reversion Features

Capture deviation from equilibrium.

| Feature | Formula | Lookback | Notes |
|---------|---------|----------|-------|
| Z-Score | `(close - mean) / std` | 20 bars | Standard score |
| Bollinger %B | `(close - lower) / (upper - lower)` | 20 bars | Position in bands |
| Distance from MA | `(close - SMA) / SMA` | 20, 50 bars | % deviation |
| Hurst Exponent | R/S analysis | 100+ bars | Mean-reverting vs trending |

### 3. Volatility Features

Capture price uncertainty and risk.

| Feature | Formula | Lookback | Notes |
|---------|---------|----------|-------|
| ATR | `EMA(max(H-L, |H-C[1]|, |L-C[1]|))` | 14 bars | True range |
| Parkinson | `√(Σ(ln(H/L))² / (4n·ln2))` | 20 bars | Range-based vol |
| Garman-Klass | `0.5(ln(H/L))² - (2ln2-1)(ln(C/O))²` | 20 bars | OHLC-based vol |
| Realized Vol | `√(Σ(returns)² × 252)` | 20 bars | Return-based vol |
| Vol of Vol | `std(rolling_vol)` | 60 bars | Volatility stability |

### 4. Microstructure Features

Capture market dynamics and liquidity.

| Feature | Formula | Notes |
|---------|---------|-------|
| Bid-Ask Spread | `(ask - bid) / mid` | Liquidity indicator |
| Trade Imbalance | `(buy_vol - sell_vol) / total_vol` | Order flow |
| VPIN | Volume-synchronized probability | Toxicity |
| Kyle's Lambda | Price impact per volume | Market depth |
| Amihud Illiquidity | `|return| / volume` | Illiquidity measure |

### 5. Volume Features

Capture trading activity patterns.

| Feature | Formula | Lookback | Notes |
|---------|---------|----------|-------|
| OBV | Cumulative signed volume | Running | On-balance volume |
| Volume Ratio | `vol / avg_vol[n]` | 20 bars | Relative volume |
| Volume Trend | `Σ(vol × sign(return))` | 10 bars | Volume direction |
| VWAP Deviation | `(close - VWAP) / VWAP` | Intraday | Fair value distance |

### 6. Cross-Asset Features

Capture inter-market relationships.

| Feature | Formula | Lookback | Notes |
|---------|---------|----------|-------|
| Correlation | `corr(ret_A, ret_B)` | 60 bars | Pair correlation |
| Beta | `cov(ret_A, ret_mkt) / var(ret_mkt)` | 60 bars | Market sensitivity |
| Spread | `price_A - hedge_ratio × price_B` | Running | Pair spread |
| Cointegration Residual | OLS residual | Rolling window | Mean-reverting spread |

## Lookback Windows

Different timeframes capture different market dynamics:

| Window | Duration | Captures |
|--------|----------|----------|
| Ultra-short | 1-5 bars | Microstructure noise |
| Short | 5-15 bars | Intraday momentum |
| Medium | 15-60 bars | Swing moves |
| Long | 60-240 bars | Trend regime |
| Very Long | 240+ bars | Structural changes |

## Feature Normalization

Raw features must be normalized before model input:

### Z-Score Normalization
```rust
fn z_score(value: f64, mean: f64, std: f64) -> f64 {
    (value - mean) / std.max(1e-10)  // Avoid division by zero
}
```

### Rank Normalization
```rust
fn rank_normalize(value: f64, window: &[f64]) -> f64 {
    let rank = window.iter().filter(|&&x| x <= value).count();
    rank as f64 / window.len() as f64
}
```

### Winsorization (Outlier Clipping)
```rust
fn winsorize(value: f64, lower: f64, upper: f64) -> f64 {
    value.max(lower).min(upper)
}
```

## Implementation Patterns

### Efficient Rolling Window

```rust
/// Ring buffer for O(1) rolling calculations
pub struct RollingWindow<const N: usize> {
    buffer: [f64; N],
    head: usize,
    sum: f64,
    sum_sq: f64,
    count: usize,
}

impl<const N: usize> RollingWindow<N> {
    pub fn push(&mut self, value: f64) {
        if self.count == N {
            let old = self.buffer[self.head];
            self.sum -= old;
            self.sum_sq -= old * old;
        }
        self.buffer[self.head] = value;
        self.sum += value;
        self.sum_sq += value * value;
        self.head = (self.head + 1) % N;
        self.count = self.count.saturating_add(1).min(N);
    }

    pub fn mean(&self) -> f64 {
        self.sum / self.count as f64
    }

    pub fn std(&self) -> f64 {
        let mean = self.mean();
        ((self.sum_sq / self.count as f64) - mean * mean).sqrt()
    }
}
```

### SIMD-Accelerated Calculations

```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// SIMD-accelerated dot product for feature computation
#[inline]
pub fn dot_product_avx(a: &[f32], b: &[f32]) -> f32 {
    unsafe {
        let mut sum = _mm256_setzero_ps();
        for i in (0..a.len()).step_by(8) {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));
            sum = _mm256_fmadd_ps(va, vb, sum);
        }
        // Horizontal sum of 8 floats
        let hi = _mm256_extractf128_ps(sum, 1);
        let lo = _mm256_castps256_ps128(sum);
        let sum128 = _mm_add_ps(hi, lo);
        let shuf = _mm_shuffle_ps(sum128, sum128, 0b10_11_00_01);
        let sum64 = _mm_add_ps(sum128, shuf);
        let shuf = _mm_movehl_ps(sum64, sum64);
        let sum32 = _mm_add_ss(sum64, shuf);
        _mm_cvtss_f32(sum32)
    }
}
```

## Common Pitfalls

### 1. Lookahead Bias
**Wrong:** Using future data in feature calculation
```rust
// BAD: Uses close[t+1] which isn't available yet
let feature = (close[t+1] - close[t]) / close[t];
```

### 2. Survivorship Bias
**Wrong:** Only using currently listed assets
**Right:** Include delisted/failed assets in historical analysis

### 3. Overfitting
**Wrong:** Too many features relative to sample size
**Right:** Use regularization, cross-validation, out-of-sample testing

### 4. Non-Stationarity
**Wrong:** Assuming constant feature distributions
**Right:** Use rolling normalization, regime detection

### 5. Data Snooping
**Wrong:** Testing many features until one works
**Right:** Pre-register hypotheses, use proper statistical correction

## Feature Selection

### Information Ratio
```rust
fn information_ratio(feature: &[f64], returns: &[f64]) -> f64 {
    let correlation = pearson_correlation(feature, returns);
    let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
    let std_return = std_dev(returns);
    correlation * mean_return / std_return
}
```

### Mutual Information
```rust
fn mutual_information(feature: &[f64], target: &[f64], bins: usize) -> f64 {
    // Discretize into bins
    let joint = joint_histogram(feature, target, bins);
    let marginal_x = marginal_histogram(feature, bins);
    let marginal_y = marginal_histogram(target, bins);

    // MI = Σ p(x,y) log(p(x,y) / (p(x)p(y)))
    let mut mi = 0.0;
    for i in 0..bins {
        for j in 0..bins {
            if joint[i][j] > 0.0 {
                mi += joint[i][j] * (joint[i][j] / (marginal_x[i] * marginal_y[j])).ln();
            }
        }
    }
    mi
}
```

## Output Format

Feature vectors are output as fixed-size arrays for efficient processing:

```rust
/// Normalized feature vector for model input
pub struct FeatureVector {
    /// Market identifier
    pub market_key: u64,
    /// Bar type that triggered this feature
    pub bar_key: i32,
    /// Timestamp in nanoseconds
    pub timestamp: u64,
    /// 256-dimensional normalized feature vector
    pub features: [f32; 256],
}
```

Feature indices are documented in `config/feature_schema.yaml`.
