# CI/CD Pipeline

## Git Workflow

```
feature branch → dev branch → master branch
                     │              │
                     ▼              ▼
              DEV deployment   PROD deployment
```

## Branches

| Branch | Environment | Auto-deploy |
|--------|-------------|-------------|
| `dev` | Development | Yes |
| `master` | Production | Yes |
| `feat/*` | Local only | No |

## Git Hooks

### Location
```
/home/claudedev/algostaking-backend/.git/hooks/post-commit
```

### What It Does
1. Detects current branch (dev or master)
2. Runs `cargo build --release`
3. Calls `algostaking restart dev|prod`

### Hook Script
```bash
#!/bin/bash
BRANCH=$(git rev-parse --abbrev-ref HEAD)

if [ "$BRANCH" = "dev" ]; then
    cargo build --release
    /usr/local/bin/algostaking restart dev
elif [ "$BRANCH" = "master" ]; then
    cargo build --release
    cp target/release/* /var/www/algostaking/prod/backend/
    /usr/local/bin/algostaking restart prod
fi
```

## Deployment Flow

### DEV Deployment
```bash
# 1. Work on feature branch
git checkout -b feat/my-feature
# ... make changes ...
cargo build --release

# 2. Merge to dev (triggers auto-deploy)
git checkout dev
git merge feat/my-feature
# Hook runs: build + restart dev services

# 3. Verify
algostaking health dev
```

### PROD Deployment
```bash
# 1. Ensure dev is stable
algostaking health dev

# 2. Merge dev to master (triggers auto-deploy)
git checkout master
git merge dev
# Hook runs: build + copy binaries + restart prod services

# 3. Verify
algostaking health prod
```

## Manual Deployment

If hooks fail or you need manual control:

```bash
# Build
cd /home/claudedev/algostaking-backend
cargo build --release

# Deploy DEV
algostaking restart dev

# Deploy PROD
cp target/release/*_service /var/www/algostaking/prod/backend/
cp target/release/pms-service /var/www/algostaking/prod/backend/
algostaking restart prod
```

## Binary Locations

| Environment | Path |
|-------------|------|
| DEV | `/home/claudedev/algostaking-backend/target/release/` |
| PROD | `/var/www/algostaking/prod/backend/` |

## Service Binary Names
```
data_ingestion_service
data_aggregation_service
data_persistence_service
strategy_feature_service
strategy_prediction_service
strategy_signal_service
configuration_service
api_service
stream_service
pms-service
oms-service
ems-service
```

## Rollback

```bash
# If deployment fails, services auto-restart with old binary
# For manual rollback:

# 1. Check git log
git log --oneline -10

# 2. Revert to previous commit
git revert HEAD

# 3. Or reset (destructive)
git reset --hard HEAD~1

# 4. Rebuild and deploy
cargo build --release
algostaking restart dev
```

## Monitoring Deployment

```bash
# Watch logs during deployment
algostaking logs dev api

# Check all services came up healthy
algostaking health dev

# Verify in Prometheus
curl -s http://localhost:9090/api/v1/targets | grep -o '"health":"[^"]*"' | sort | uniq -c
```
