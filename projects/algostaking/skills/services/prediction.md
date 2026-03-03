# Service: prediction

## Required Reading
1. `.claude/skills/pipelines/strategy.md`
2. `.claude/skills/crates/types.md` - PredictionData
3. `.claude/skills/crates/keys.md` - FnoHeadKey

## Purpose

Fourier Neural Operator (FNO) inference for multi-horizon trading predictions with atomic model hot-swap.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point |
| `src/fno/encoder.rs` | Input projection |
| `src/fno/spectral.rs` | Spectral convolution layers |
| `src/fno/decoder.rs` | Output projection |
| `src/inference.rs` | Inference loop |
| `src/checkpoint.rs` | Model loading, hot-swap |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `model.checkpoint_dir` | `/var/lib/algostaking/checkpoints` | Model weights |
| `model.batch_size` | `32` | Inference batch size |
| `inference.use_gpu` | `false` | GPU acceleration |
| `inference.threads` | `4` | CPU threads |
| `zmq.sub_endpoint` | `tcp://127.0.0.1:5557` | Feature input |
| `zmq.pub_endpoint` | `tcp://0.0.0.0:5558` | Prediction output |

## ZMQ Connections

| Direction | Port | Topic | Description |
|-----------|------|-------|-------------|
| IN | 5557 | Binary 14-byte | Features |
| OUT | 5558 | Binary 14-byte | Predictions |
| OUT | 5559 | String | Model checkpoints |

## FNO Architecture

```
Input: [batch, time, features]
       ↓
   Encoder (Linear projection)
       ↓
   Spectral Conv × N (Fourier space)
       ↓
   Decoder (Linear projection)
       ↓
Output: [magnitude, retracement, horizon]
```

## Head Key Strategy

FnoHeadKey is **venue-agnostic**: `[bar_type:16][base_asset:16]`
- BTC head works for Binance, Bybit, OKX, etc.
- Reduces model count, shares cross-venue patterns

## Testing

```bash
cargo build --release -p prediction
cargo test --release -p prediction

# Profile inference
perf record ./target/release/prediction
perf report
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| High latency | CPU contention | Increase threads, use batch |
| Model drift | Stale checkpoint | Check checkpoint age |
