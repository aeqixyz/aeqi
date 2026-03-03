# Secrets Management

## Environment Files

Secrets are stored in environment files, loaded by systemd services.

| Environment | File |
|-------------|------|
| DEV | `/etc/algostaking/backend-dev.env` |
| PROD | `/etc/algostaking/backend-prod.env` |

## File Format

```bash
# /etc/algostaking/backend-dev.env
RUST_LOG=info
ENVIRONMENT=development
DATABASE_URL=postgresql://algo_dev:PASSWORD@localhost:5432/algostaking_dev
JWT_SECRET=base64_encoded_secret
CORS_ORIGINS=https://dev.app.algostaking.com,http://localhost:3000
```

## Secrets Inventory

| Secret | Used By | Location |
|--------|---------|----------|
| Database password | All DB services | `DATABASE_URL` in env file |
| JWT secret | api_service | `JWT_SECRET` in env file |
| Exchange API keys | ingestion | Service config or env |

## Accessing Secrets

### View (requires sudo)
```bash
sudo cat /etc/algostaking/backend-dev.env
```

### Edit
```bash
sudo nano /etc/algostaking/backend-dev.env
# Then restart services
algostaking restart dev
```

## Database Credentials

### DEV
- User: `algo_dev`
- Database: `algostaking_dev`
- Password: In `/etc/algostaking/backend-dev.env`

### PROD
- User: `algo_prod`
- Database: `algostaking_prod`
- Password: In `/etc/algostaking/backend-prod.env`

### Change Database Password
```bash
# 1. Update PostgreSQL
sudo -u postgres psql -c "ALTER USER algo_dev PASSWORD 'newpassword';"

# 2. Update env file
sudo nano /etc/algostaking/backend-dev.env
# Change DATABASE_URL

# 3. Restart services
algostaking restart dev
```

## JWT Secret

Used by API service for authentication tokens.

### Generate New Secret
```bash
openssl rand -base64 32
```

### Update
```bash
sudo nano /etc/algostaking/backend-dev.env
# Update JWT_SECRET=new_secret
algostaking restart dev api
```

## File Permissions

```bash
# Environment files should be readable only by root and service user
sudo chown root:root /etc/algostaking/*.env
sudo chmod 600 /etc/algostaking/*.env
```

## Systemd Integration

Services load env file via `EnvironmentFile=` directive:

```ini
# In /etc/systemd/system/algostaking-dev-api.service
[Service]
EnvironmentFile=/etc/algostaking/backend-dev.env
```

## Never Commit Secrets

Files that should NEVER be committed:
- `/etc/algostaking/*.env`
- Any `.env` files
- `config/**/secrets.yaml`
- API keys, passwords, tokens

## Grafana Credentials

| Instance | Default | Reset Command |
|----------|---------|---------------|
| DEV | admin/admin | `sudo grafana-cli admin reset-admin-password newpass` |
| PROD | admin/admin | `sudo grafana-cli --config /etc/grafana-prod/grafana.ini admin reset-admin-password newpass` |

## Certbot/SSL

Certificates managed by certbot, auto-renewed.
```bash
# List certificates
sudo certbot certificates

# Certificates stored in
/etc/letsencrypt/live/
```
