# Latency Debugging Guide

## Quick Diagnostics

### 1. Check Service Latency via Metrics

```bash
# Current P99 latency for ingestion
curl -s localhost:9000/metrics | grep -E "parse_latency_ns.*P99|histogram_quantile"

# Current P99 for aggregation
curl -s localhost:9001/metrics | grep process_latency

# Check all services
for port in 9000 9001 9002 9003 9004 9008; do
  echo "=== Port $port ==="
  curl -s localhost:$port/metrics | grep -E "latency.*stat=\"max\""
done
```

### 2. Grafana Dashboard Check

1. Open Grafana: https://grafana.algostaking.com (prod) or https://dev.grafana.algostaking.com (dev)
2. Check "Overview" dashboard for service health
3. Check service-specific dashboards for latency percentiles

### 3. Quick Health Check

```bash
# Service uptime and basic metrics
algostaking health dev

# Check if services are responding
for port in 9000 9001 9002 9003 9004 9008 9012 9013 9009; do
  curl -s -o /dev/null -w "%{http_code} " localhost:$port/health
done
```

## Deep Diagnostics

### CPU Profiling with perf

```bash
# Record 30 seconds of CPU activity
sudo perf record -F 99 -p $(pgrep -f data_ingestion) -g -- sleep 30

# Generate report
sudo perf report --stdio

# Generate flamegraph
sudo perf script | stackcollapse-perf.pl | flamegraph.pl > ingestion.svg
```

### Syscall Analysis

```bash
# Count syscalls by type
sudo strace -c -p $(pgrep -f data_ingestion)

# Trace specific syscalls with timing
sudo strace -T -e read,write,recvmsg,sendmsg -p $(pgrep -f data_ingestion)
```

### Cache Analysis

```bash
# Check cache misses
sudo perf stat -e cache-misses,cache-references,L1-dcache-load-misses -p $(pgrep -f data_ingestion) sleep 10

# Detailed cache simulation (slow)
valgrind --tool=cachegrind --branch-sim=yes ./target/release/data_ingestion
```

### Memory Allocation Analysis

```bash
# Heap profiling with DHAT
RUSTFLAGS="-C target-cpu=native" cargo build --release --features dhat
./target/release/data_ingestion
# Check dhat-heap.json output

# Alternative: Use mimalloc stats
MIMALLOC_SHOW_STATS=1 ./target/release/data_ingestion
```

## Common Latency Issues

### 1. Lock Contention

**Symptoms:**
- High CPU usage but low throughput
- Latency spikes under load
- `perf` shows time in `pthread_mutex_lock`

**Diagnosis:**
```bash
# Check for lock contention
sudo perf record -e sched:sched_switch -p $(pgrep -f data_ingestion) -- sleep 5
sudo perf report
```

**Solutions:**
- Replace Mutex with RwLock if reads dominate
- Use lock-free data structures (atomics, crossbeam)
- Reduce critical section size
- Use sharding (DashMap)

### 2. Memory Allocation in Hot Path

**Symptoms:**
- Periodic latency spikes
- High allocation rate in heap profiler
- `perf` shows time in malloc/free

**Diagnosis:**
```bash
# Check allocation rate
MIMALLOC_SHOW_STATS=1 ./target/release/data_ingestion &
sleep 30
kill -USR1 $!  # Prints stats
```

**Solutions:**
- Pre-allocate buffers at startup
- Use arenas for temporary allocations
- Reuse buffers instead of creating new ones
- Use `SmallVec` for small collections

### 3. Cache Misses

**Symptoms:**
- High L1/L2/L3 cache miss rate
- Latency increases with data size
- `perf stat` shows cache-misses

**Diagnosis:**
```bash
sudo perf stat -e L1-dcache-load-misses,L1-dcache-loads \
               -e LLC-load-misses,LLC-loads \
               -p $(pgrep -f data_ingestion) sleep 10
```

**Solutions:**
- Pack hot data together (struct of arrays vs array of structs)
- Align data to cache lines (64 bytes)
- Prefetch data before use
- Reduce pointer chasing

### 4. Syscall Overhead

**Symptoms:**
- Many small I/O operations
- High syscall count in strace
- Time spent in kernel

**Diagnosis:**
```bash
sudo strace -c -p $(pgrep -f data_ingestion)
```

**Solutions:**
- Batch I/O operations
- Use io_uring for async I/O
- Use larger buffers
- Reduce logging frequency

### 5. GC/Allocator Pauses

**Symptoms:**
- Periodic latency spikes (mimalloc arena resets)
- Spikes correlate with memory pressure

**Diagnosis:**
```bash
# Monitor memory usage
watch -n 1 'ps -p $(pgrep -f data_ingestion) -o rss,vsz'
```

**Solutions:**
- Tune mimalloc arena sizes
- Pre-allocate and reuse memory
- Reduce allocation frequency
- Use arena allocators for temporary data

## Benchmarking

### Micro-benchmarks with Criterion

```rust
// In benches/latency.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn parse_benchmark(c: &mut Criterion) {
    let data = include_bytes!("../testdata/trade.json");

    c.bench_function("parse_trade", |b| {
        b.iter(|| parse_trade(data))
    });
}

criterion_group!(benches, parse_benchmark);
criterion_main!(benches);
```

```bash
cargo bench --bench latency
```

### End-to-End Latency Measurement

```rust
// Measure from ingestion to signal
let start = std::time::Instant::now();
// ... processing ...
let latency_ns = start.elapsed().as_nanos();

METRICS.total_latency_histogram.record(latency_ns as u64);
```

## Tracing

### tokio-console

```bash
# Install
cargo install tokio-console

# Run with console support
RUSTFLAGS="--cfg tokio_unstable" cargo build --release
tokio-console
```

### Distributed Tracing (if enabled)

```bash
# View traces in Jaeger
open http://localhost:16686
```

## Performance Regression Detection

### CI Integration

```bash
# Run benchmarks and compare to baseline
cargo bench -- --save-baseline new
critcmp baseline new
```

### Monitoring Alerts

Set up Grafana alerts for:
- P99 latency > 2x target
- P99.9 latency > 5x target
- Error rate > 0.1%
- Throughput drop > 20%

## Quick Reference

| Tool | Command | Use Case |
|------|---------|----------|
| perf record | `sudo perf record -F 99 -p PID -g -- sleep 30` | CPU profiling |
| perf stat | `sudo perf stat -e cache-misses -p PID sleep 10` | Cache analysis |
| strace | `sudo strace -c -p PID` | Syscall overhead |
| flamegraph | `perf script \| flamegraph.pl > out.svg` | Visual profiling |
| criterion | `cargo bench` | Micro-benchmarks |
| mimalloc stats | `MIMALLOC_SHOW_STATS=1 ./binary` | Allocation stats |
| tokio-console | `tokio-console` | Async task debugging |

## Latency Target Reference

| Metric | Green | Yellow | Red |
|--------|-------|--------|-----|
| Parse P99 | <2.5us | 2.5-5us | >5us |
| Aggregation P99 | <5us | 5-10us | >10us |
| Feature P99 | <20us | 20-50us | >50us |
| Prediction P99 | <100us | 100-200us | >200us |
| End-to-end P99 | <200us | 200-500us | >500us |
