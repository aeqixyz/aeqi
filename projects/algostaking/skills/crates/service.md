# Crate: service

## Purpose

Shared service infrastructure for AlgoStaking backend services. Provides unified configuration loading and graceful shutdown handling.

## Public API

### Configuration Loading

```rust
use service::{
    load_config,       // Load from default path or CONFIG_PATH env var
    load_config_from,  // Load from specific path
    database_url,      // Get DATABASE_URL with fallback
    ConfigError,       // Configuration errors
};
```

### Shutdown Handling

```rust
use service::{
    shutdown_signal,       // Future that completes on SIGTERM/SIGINT
    is_shutdown,          // Hot-path-safe shutdown check (AtomicBool)
    request_shutdown,     // Programmatically trigger shutdown
    spawn_shutdown_handler, // Spawn background shutdown listener
    SHUTDOWN,             // Global AtomicBool
};
```

### Common Config Types

```rust
use service::{
    ServerConfig,         // host, port, metrics_port
    CorsConfig,          // allowed_origins, max_age
    ReconnectionConfig,  // initial_delay, max_delay, max_attempts
    RegistryClientConfig, // endpoint, timeout
};
```

## Canonical Usage

### Pattern 1: Load Service Configuration

```rust
use service::load_config;
use serde::Deserialize;

#[derive(Deserialize)]
struct MyServiceConfig {
    server: ServerConfig,
    zmq: ZmqConfig,
    #[serde(default)]
    limits: LimitsConfig,
}

#[derive(Deserialize, Default)]
struct LimitsConfig {
    #[serde(default = "default_batch_size")]
    batch_size: usize,
}

fn default_batch_size() -> usize { 64 }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Loads from config/service.yaml or CONFIG_PATH env var
    let config: MyServiceConfig = load_config("config/service.yaml")?;

    println!("Starting on port {}", config.server.port);
    Ok(())
}
```

### Pattern 2: Graceful Shutdown

```rust
use service::{shutdown_signal, is_shutdown};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::select! {
        _ = run_service() => {},
        _ = shutdown_signal() => {
            println!("Received shutdown signal");
        }
    }

    // Cleanup...
    println!("Service stopped gracefully");
    Ok(())
}

async fn run_service() {
    loop {
        // Hot-path safe shutdown check
        if is_shutdown() {
            break;
        }

        // Do work...
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
```

### Pattern 3: Database URL with Fallback

```rust
use service::database_url;

async fn connect_db() -> Result<Pool, Error> {
    // Checks DATABASE_URL env var, falls back to config
    let url = database_url()?;
    Pool::connect(&url).await
}
```

### Pattern 4: Load Config with Environment Override

```rust
use service::load_config;

// Config file: config/service.yaml
// Override with: CONFIG_PATH=/etc/myservice/config.yaml

let config: MyConfig = load_config("config/service.yaml")?;
// If CONFIG_PATH is set, loads from that path instead
```

### Pattern 5: Common Server Config Pattern

```rust
use service::ServerConfig;
use ports::METRICS_AGGREGATION;

#[derive(Deserialize)]
struct Config {
    #[serde(default)]
    server: ServerConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            metrics_port: METRICS_AGGREGATION,
        }
    }
}
```

### Pattern 6: Spawn Background Shutdown Handler

```rust
use service::spawn_shutdown_handler;

#[tokio::main]
async fn main() {
    // Spawns task that sets SHUTDOWN on SIGTERM/SIGINT
    spawn_shutdown_handler();

    // Now is_shutdown() will return true after signal
    while !is_shutdown() {
        do_work().await;
    }
}
```

## Anti-Patterns

### DON'T: Hardcode Configuration

```rust
// WRONG: Hardcoded values
const BATCH_SIZE: usize = 64;
const PORT: u16 = 8080;

// RIGHT: Load from config
#[derive(Deserialize)]
struct Config {
    #[serde(default = "default_batch_size")]
    batch_size: usize,
    port: u16,
}
```

### DON'T: Use std::process::exit() for Shutdown

```rust
// WRONG: Abrupt exit, no cleanup
if should_stop {
    std::process::exit(0);  // No cleanup!
}

// RIGHT: Graceful shutdown
if should_stop {
    request_shutdown();  // Sets flag, allows cleanup
}
```

### DON'T: Block on Shutdown Check

```rust
// WRONG: Blocking call in hot path
loop {
    if shutdown_signal().await {  // Blocks!
        break;
    }
    process();
}

// RIGHT: Non-blocking check
loop {
    if is_shutdown() {  // AtomicBool load
        break;
    }
    process();
}
```

### DON'T: Skip Serde Defaults

```rust
// WRONG: Required fields break backward compat
#[derive(Deserialize)]
struct Config {
    new_field: String,  // Old configs will fail!
}

// RIGHT: Use serde defaults
#[derive(Deserialize)]
struct Config {
    #[serde(default = "default_new_field")]
    new_field: String,
}
```

## Violation Detection

```bash
# Find hardcoded magic numbers that should be config
rg "const\s+\w+:\s*(usize|u16|u32|i32)" --type rust services/ | grep -v "TOPIC_SIZE\|MAX_"

# Find std::process::exit usage
rg "std::process::exit\|process::exit" --type rust services/

# Find missing serde defaults
rg "#\[derive.*Deserialize" -A 10 --type rust services/ | grep -E "pub \w+:" | grep -v "default\|Option"

# Find services not using service crate
for svc in services/*/*; do
    if [ -f "$svc/Cargo.toml" ]; then
        if ! grep -q "^service\s*=" "$svc/Cargo.toml"; then
            echo "Missing service dependency: $svc"
        fi
    fi
done
```

## Migration Guide

### Adding New Config Fields (Backward Compatible)

```rust
// 1. Add field with serde default
#[derive(Deserialize)]
struct Config {
    existing_field: String,

    #[serde(default = "default_new_field")]
    new_field: NewType,
}

fn default_new_field() -> NewType {
    NewType::default()
}

// 2. Update config/service.yaml with new field
// (Optional - default will be used if missing)

// 3. Wire through to code
impl Service {
    fn new(config: &Config) -> Self {
        Self {
            setting: config.new_field.clone(),
        }
    }
}
```

### From Custom Shutdown to service Crate

```rust
// Before (custom implementation)
static SHUTDOWN: AtomicBool = AtomicBool::new(false);
tokio::spawn(async move {
    signal::ctrl_c().await.unwrap();
    SHUTDOWN.store(true, Ordering::SeqCst);
});

// After (use service crate)
use service::{spawn_shutdown_handler, is_shutdown};
spawn_shutdown_handler();
// ...
if is_shutdown() { break; }
```

## Cross-References

- **Used by:** All services for config and shutdown
- **Related skills:** `systemd.md` (service management)
- **Code location:** `crates/service/src/`

## Key Files

| File | Purpose |
|------|---------|
| `config.rs` | load_config, database_url |
| `common_config.rs` | ServerConfig, CorsConfig, etc. |
| `shutdown.rs` | shutdown_signal, is_shutdown, SHUTDOWN |

## Config File Template

```yaml
# config/service.yaml

server:
  host: "0.0.0.0"
  port: 8080
  metrics_port: 9000

zmq:
  sub_endpoint: "tcp://127.0.0.1:5555"
  pub_endpoint: "tcp://0.0.0.0:5556"

limits:
  batch_size: 64
  max_connections: 10000

# Environment-specific overrides:
# CONFIG_PATH=/etc/algostaking/prod/service.yaml
# DATABASE_URL=postgresql://user:pass@host/db
```
