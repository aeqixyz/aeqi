# Pipeline: Gateway

## Services

| Service | Port | Purpose |
|---------|------|---------|
| **configuration** | 9007 | Service configuration distribution |
| **api** | 8082 (HTTP), 9010 (metrics) | REST API for account management, auth, TOTP |
| **stream** | 8081 (WS), 9011 (metrics) | High-performance WebSocket gateway |

## Data Flow

```
                    ┌─────────────────────────────────────┐
                    │         CONFIGURATION               │
                    │         (Port 9007)                 │
                    │                                     │
                    │  • Registry (assets, venues, bars)  │
                    │  • Subscriptions                    │
                    │  • Dynamic config                   │
                    └───────────────┬─────────────────────┘
                                    │ ZMQ REP/PUB 5560
                                    ▼
    ┌───────────────────────────────────────────────────────────────┐
    │                   Internal Services                            │
    │  (ingestion, aggregation, feature, prediction, signal, etc.)  │
    └───────────────────────────────────────────────────────────────┘


                    ┌─────────────────────────────────────┐
                    │              API                    │
                    │         (Port 8082)                 │
                    │                                     │
                    │  • User authentication (JWT)        │
                    │  • Account management               │
                    │  • TOTP 2FA                         │
                    │  • Fund/strategy CRUD               │
                    └─────────────────────────────────────┘
                                    │ REST over HTTPS
                                    ▼
                              Web Clients


From Internal Services (5555-5561)
          │
          │ Tick, Bar, Feature, Signal
          ▼
┌───────────────────┐
│      STREAM       │
│  (Port 8081)      │
│                   │
│  • Sub-μs latency │
│  • Binary frames  │
│  • Channel subs   │
│  • Rate limiting  │
└─────────────────────┘
          │ WebSocket (wss://)
          ▼
     Web Clients
```

## ZMQ Topics (Configuration)

| Direction | Port | Format | Purpose |
|-----------|------|--------|---------|
| Services → Config | 5560 REQ | String | Request registry data |
| Config → Services | 5560 REP | FlatBuffer | Registry response |
| Config → Services | 5560 PUB | String prefix | Subscription updates |

### Subscription Topics

```rust
// Market subscriptions
SUBSCRIPTION_MARKET_ADD    // "sub.market.add"
SUBSCRIPTION_MARKET_REMOVE // "sub.market.remove"

// Bar subscriptions
SUBSCRIPTION_BAR_ADD       // "sub.bar.add"
SUBSCRIPTION_BAR_REMOVE    // "sub.bar.remove"

// Feature subscriptions
SUBSCRIPTION_FEATURE_ADD   // "sub.feature.add"
SUBSCRIPTION_FEATURE_REMOVE // "sub.feature.remove"

// Strategy subscriptions (fund-scoped)
SUBSCRIPTION_STRATEGY_ADD  // "sub.strategy.add"
SUBSCRIPTION_STRATEGY_REMOVE // "sub.strategy.remove"
```

## Latency Targets

| Operation | Target | Typical | Notes |
|-----------|--------|---------|-------|
| WS frame send | <1μs | 500ns | Binary protocol |
| REST API | <10ms | 2-5ms | With DB query |
| Config lookup | <1ms | 100μs | Cached |
| Auth (JWT) | <1ms | 200μs | In-memory verify |

## Key Patterns

### Configuration: Registry Service

```rust
use types::fb_asset_generated::registry::Asset;

struct RegistryService {
    assets: HashMap<u16, Asset>,
    venues: HashMap<u16, Venue>,
    bar_types: HashMap<u16, BarType>,
}

impl RegistryService {
    // REQ/REP pattern for queries
    async fn handle_request(&self, req: &[u8]) -> Vec<u8> {
        match parse_request(req) {
            Request::GetAssets => self.serialize_assets(),
            Request::GetVenues => self.serialize_venues(),
            // ...
        }
    }

    // PUB pattern for updates
    async fn broadcast_subscription(&self, sub: &Subscription) {
        let topic = match sub.action {
            Action::Add => SUBSCRIPTION_MARKET_ADD,
            Action::Remove => SUBSCRIPTION_MARKET_REMOVE,
        };
        self.pub_socket.send(&[topic, &sub.serialize()]).await;
    }
}
```

### API: JWT Authentication

```rust
use axum::{extract::State, middleware::Next, response::Response};
use jsonwebtoken::{decode, Validation};

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, Error> {
    let token = req.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(Error::Unauthorized)?;

    let claims = decode::<Claims>(token, &state.jwt_key, &Validation::default())?;

    // Attach user to request
    req.extensions_mut().insert(claims.claims);

    Ok(next.run(req).await)
}
```

### API: TOTP 2FA

```rust
use totp_rs::{Algorithm, TOTP, Secret};

fn verify_totp(secret: &str, code: &str) -> bool {
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,       // digits
        1,       // skew (allow 1 period before/after)
        30,      // step (seconds)
        Secret::Encoded(secret.to_string()).to_bytes().unwrap(),
    ).unwrap();

    totp.check_current(code).unwrap_or(false)
}
```

### Stream: Sub-Microsecond WebSocket

```rust
use fastwebsockets::{Frame, OpCode, WebSocket};
use core_affinity::CoreId;
use ringbuf::HeapRb;

struct StreamGateway {
    // CPU-pinned worker threads
    workers: Vec<Worker>,
    // SPSC ring buffer per connection
    queues: HashMap<ConnectionId, Producer<Frame>>,
}

impl StreamGateway {
    fn spawn_writer_thread(core: CoreId) -> JoinHandle<()> {
        std::thread::spawn(move || {
            // Pin to specific CPU core
            core_affinity::set_for_current(core);

            // Hot loop - no allocations
            loop {
                if let Some(frame) = consumer.pop() {
                    socket.write_frame(frame).unwrap();
                }
            }
        })
    }
}
```

### Stream: Channel Subscriptions

```rust
use types::Channel;

struct Subscription {
    channels: Vec<Channel>,
}

// Channel hierarchy
impl Channel {
    // Tick: market_key only
    fn tick(market_key: i64) -> Self {
        Self { market_key, bar_key: None, feature_id: None }
    }

    // Bar: market_key + bar_key
    fn bar(market_key: i64, bar_key: i32) -> Self {
        Self { market_key, bar_key: Some(bar_key), feature_id: None }
    }

    // Signal: market_key + bar_key + feature_id
    fn signal(market_key: i64, bar_key: i32, feature_id: u16) -> Self {
        Self { market_key, bar_key: Some(bar_key), feature_id: Some(feature_id) }
    }
}
```

### Stream: Binary Frame Protocol

```rust
// Frame format: [type:1][length:2][payload:N]
enum FrameType {
    Tick = 1,
    Bar = 2,
    Feature = 3,
    Signal = 4,
    Snapshot = 10,
    Error = 255,
}

fn encode_frame(frame_type: FrameType, payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u16;
    let mut frame = Vec::with_capacity(3 + payload.len());
    frame.push(frame_type as u8);
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(payload);
    frame
}
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| WS high latency | Queue backup | Check SPSC ring size |
| JWT expired | Token not refreshed | Client-side refresh logic |
| Config stale | Cache not invalidated | Check PUB subscription |
| Rate limited | Too many requests | Implement backoff |

## Required Crate Skills

Before modifying this pipeline, read:
1. `.claude/skills/crates/types.md` - FlatBuffer registry types
2. `.claude/skills/crates/keys.md` - Channel key composition
3. `.claude/skills/crates/ports.md` - Gateway port constants

## Service-Specific Skills

- `.claude/skills/services/configuration.md`
- `.claude/skills/services/api.md`
- `.claude/skills/services/stream.md`

## Monitoring

| Service | Key Metrics |
|---------|-------------|
| Configuration | `registry_requests_total`, `subscriptions_active`, `cache_hits` |
| API | `requests_total`, `auth_failures`, `latency_ms` |
| Stream | `connections_active`, `frames_sent_total`, `frame_latency_ns` |

## Configuration

### Configuration Service (`config/dev/configuration.yaml`)
```yaml
zmq:
  rep_endpoint: "tcp://0.0.0.0:5560"
  pub_endpoint: "tcp://0.0.0.0:5560"
database:
  url: "postgresql://algo_dev@localhost/algostaking_dev"
cache:
  ttl_seconds: 300
```

### API (`config/dev/api.yaml`)
```yaml
server:
  host: "0.0.0.0"
  port: 8082
  metrics_port: 9010
auth:
  jwt_secret_file: "/etc/algostaking/secrets/jwt_secret"
  token_expiry_hours: 24
cors:
  allowed_origins:
    - "https://dev.app.algostaking.com"
database:
  url: "postgresql://algo_dev@localhost/algostaking_dev"
  pool_size: 10
```

### Stream (`config/dev/stream.yaml`)
```yaml
server:
  host: "0.0.0.0"
  port: 8081
  metrics_port: 9011
zmq:
  tick_endpoint: "tcp://127.0.0.1:5555"
  bar_endpoint: "tcp://127.0.0.1:5556"
  signal_endpoint: "tcp://127.0.0.1:5561"
performance:
  worker_threads: 4
  ring_buffer_size: 65536
  cpu_affinity: [0, 1, 2, 3]
rate_limit:
  connections_per_ip: 10
  messages_per_second: 1000
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/auth/login` | Login, returns JWT |
| POST | `/api/auth/register` | Create account |
| POST | `/api/auth/totp/setup` | Setup 2FA |
| POST | `/api/auth/totp/verify` | Verify 2FA code |
| GET | `/api/user/profile` | Get user profile |
| GET | `/api/funds` | List user funds |
| POST | `/api/funds` | Create fund |
| GET | `/api/strategies` | List strategies |
| POST | `/api/subscriptions` | Subscribe to signals |

## WebSocket Messages

### Client → Server
```json
{"type": "subscribe", "channels": [{"market_key": 123, "bar_key": 456}]}
{"type": "unsubscribe", "channels": [...]}
{"type": "ping"}
```

### Server → Client
```
Binary frames: [type:1][len:2][payload:N]
```
