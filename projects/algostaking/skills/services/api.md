# Service: api

## Required Reading
1. `.claude/skills/pipelines/gateway.md`

## Purpose

REST API for user account management, authentication, and fund/strategy CRUD.

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, Axum router |
| `src/routes/auth.rs` | Login, register, TOTP |
| `src/routes/user.rs` | Profile, settings |
| `src/routes/funds.rs` | Fund CRUD |
| `src/routes/strategies.rs` | Strategy management |
| `src/middleware/` | Auth, CORS, rate limiting |
| `src/jwt.rs` | JWT encode/decode |
| `src/totp.rs` | TOTP 2FA |

## Configuration

| Key | Default | Description |
|-----|---------|-------------|
| `server.port` | `8082` | HTTP port |
| `server.metrics_port` | `9010` | Metrics port |
| `auth.jwt_secret_file` | `/etc/algostaking/secrets/jwt_secret` | JWT secret |
| `auth.token_expiry_hours` | `24` | Token lifetime |
| `cors.allowed_origins` | `[https://dev.app.algostaking.com]` | CORS origins |

## Endpoints

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

## Authentication Flow

1. POST /auth/login → Check password (argon2)
2. If 2FA enabled → Return `{requires_totp: true}`
3. POST /auth/totp/verify → Verify TOTP code
4. Return JWT token
5. Use `Authorization: Bearer <token>` for subsequent requests

## Testing

```bash
cargo build --release -p api
cargo test --release -p api

# Manual test
curl -X POST https://dev.api.algostaking.com/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","password":"..."}'
```

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| 401 Unauthorized | Invalid/expired JWT | Refresh token |
| CORS error | Origin not whitelisted | Add to cors.allowed_origins |
