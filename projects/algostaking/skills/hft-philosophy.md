# HFT Philosophy & Principles

## Core Tenets

1. **Latency = Money**: Every microsecond of delay costs alpha. In HFT, being first matters.
2. **Consistency > Speed**: Low jitter matters more than low average latency. A system with 1us avg and 100us P99 loses to 2us avg with 3us P99.
3. **Zero Allocation Hot Path**: malloc/free in hot path = death. Pre-allocate everything, use arenas.
4. **Cache-Friendly Design**: L1 cache hit = 1ns, L2 = 4ns, L3 = 12ns, RAM = 100ns. Design for cache.
5. **Lock-Free Everything**: Mutex in hot path = 25us+ latency spike. Use atomics, lock-free queues.

## Latency Budget

| Stage | Target | Unacceptable | Notes |
|-------|--------|--------------|-------|
| WebSocket receive | <10us | >100us | Network bound |
| JSON parse (simdjson) | <500ns | >2us | Zero-copy parsing |
| FlatBuffer build | <200ns | >1us | Pre-allocated buffers |
| ZMQ publish | <500ns | >2us | Kernel bypass if possible |
| Feature calculation | <5us | >20us | SIMD vectorized |
| FNO inference | <100us | >500us | GPU or optimized CPU |
| Signal generation | <10us | >50us | Simple aggregation |
| Order submission | <100us | >500us | Network bound |

## Design Patterns

### Hot Path Rules

**NEVER do this in hot path:**
- Allocate memory (malloc, Box::new, Vec::push that grows)
- Take locks (Mutex, RwLock in write mode)
- Make syscalls (file I/O, network I/O without io_uring)
- Branch unpredictably (use branchless comparisons)
- Log synchronously (use async logging or sampling)

**ALWAYS do this:**
- Pre-allocate all buffers at startup
- Use atomic operations (Relaxed ordering when possible)
- Keep hot data in contiguous memory (cache friendly)
- Measure P99.9, not just P99

### Memory Patterns

```rust
// BAD: Allocates on every call
fn process(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();  // ALLOCATION!
    // ...
    result
}

// GOOD: Reuse pre-allocated buffer
fn process(data: &[u8], buffer: &mut Vec<u8>) {
    buffer.clear();  // No allocation
    // ...
}
```

### Lock-Free Patterns

```rust
// BAD: Mutex in hot path
let value = mutex.lock().unwrap().get_value();

// GOOD: Atomic operations
let value = atomic_value.load(Ordering::Relaxed);

// GOOD: Lock-free queue
let item = spsc_queue.try_pop();
```

### Cache-Friendly Patterns

```rust
// BAD: Pointer chasing (cache miss per element)
struct Node {
    value: u64,
    next: Option<Box<Node>>,  // Cache miss!
}

// GOOD: Contiguous memory (cache friendly)
struct Data {
    values: Vec<u64>,  // All in one cache line
}
```

## Measurement Rules

1. **Measure P99.9, not just P99**: 1-in-1000 bad events kill HFT performance
2. **Measure under load**: Idle benchmarks lie
3. **Measure end-to-end**: Component benchmarks miss integration costs
4. **Track jitter**: Standard deviation matters as much as mean
5. **Use nanosecond precision**: Microseconds hide important details

## SLO Targets

| Metric | SLO | Error Budget |
|--------|-----|--------------|
| Parse latency P99 | <2.5us | 0.1% above |
| Aggregation P99 | <5us | 0.1% above |
| Feature calc P99 | <20us | 0.1% above |
| End-to-end P99 | <100us | 0.1% above |
| Message loss | 0% | 0 messages |
| Uptime | 99.99% | 52 min/year |

## Common Mistakes

### 1. Premature Optimization
Don't optimize before measuring. Profile first, then optimize the actual bottleneck.

### 2. Wrong Ordering
Using `Ordering::SeqCst` everywhere when `Ordering::Relaxed` suffices:
```rust
// Usually fine for metrics:
counter.fetch_add(1, Ordering::Relaxed);
```

### 3. Ignoring Tail Latency
A system is only as fast as its slowest path. One bad P99.9 event per 1000 can destroy trading performance.

### 4. Over-Engineering
Simple is fast. Complex abstractions add indirection and cache misses.

## Tools

| Tool | Use Case |
|------|----------|
| `perf` | CPU profiling, cache misses |
| `flamegraph` | Visual call stack analysis |
| `strace -c` | Syscall overhead |
| `criterion` | Rust micro-benchmarks |
| `dhat` | Heap allocation profiling |
| `valgrind --tool=cachegrind` | Cache simulation |

## References

- "What Every Programmer Should Know About Memory" - Ulrich Drepper
- "The Art of HFT" - Various whitepapers
- Rust Performance Book: https://nnethercote.github.io/perf-book/
