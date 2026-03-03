# Clean Slate Reset

Full database purge + service restart procedure for the dev environment.
Use when you need a completely fresh state (after schema changes, data corruption, or validation).

## Procedure

### 1. Stop All Services

```bash
algostaking stop dev
```

Wait for all 12 services to confirm stopped before touching the database.

### 2. Drop and Recreate Database

```bash
sudo -u postgres psql -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = 'algostaking_dev' AND pid <> pg_backend_pid();"
sudo -u postgres psql -c "DROP DATABASE IF EXISTS algostaking_dev;"
sudo -u postgres psql -c "CREATE DATABASE algostaking_dev OWNER algo_dev;"
```

### 3. Enable Extensions

```bash
sudo -u postgres psql -d algostaking_dev -c "CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;"
```

### 4. Create Schemas

```bash
sudo -u postgres psql -d algostaking_dev -c "
  CREATE SCHEMA IF NOT EXISTS trading;
  CREATE SCHEMA IF NOT EXISTS user_management;
"
```

### 5. Copy User Management + Trading Schemas from Prod

These schemas are managed by the API service and have no schema SQL files:

```bash
sudo -u postgres pg_dump -d algostaking_prod --schema-only -n user_management -n trading \
  | sudo -u postgres psql -d algostaking_dev
```

### 6. Apply Schema Files

```bash
cd /home/claudedev/algostaking-backend/infrastructure/schema
for f in registry_tables.sql subscription_tables.sql portfolio_tables.sql \
         signal_tables.sql trading_tables.sql positions_table.sql open_trades_table.sql; do
  echo "=== $f ==="
  sudo -u postgres psql -d algostaking_dev -f "$f"
done
```

Ignore TimescaleDB compression/retention policy errors (Apache license limitation).

### 7. Apply Migrations

```bash
cd /home/claudedev/algostaking-backend/infrastructure/migrations
for f in $(ls *.sql | sort); do
  echo "=== $f ==="
  sudo -u postgres psql -d algostaking_dev -f "$f"
done
```

Migrations referencing dropped tables (intents, trades) will show harmless errors.

### 8. Fix Ownership

Tables created by `postgres` superuser need ownership transferred to `algo_dev`:

```bash
sudo -u postgres psql -d algostaking_dev -c "
DO \$\$
DECLARE r RECORD;
BEGIN
  FOR r IN SELECT schemaname, tablename FROM pg_tables
           WHERE schemaname IN ('public','trading','user_management') LOOP
    EXECUTE 'ALTER TABLE ' || quote_ident(r.schemaname) || '.' || quote_ident(r.tablename) || ' OWNER TO algo_dev';
  END LOOP;
  FOR r IN SELECT schemaname, sequencename FROM pg_sequences
           WHERE schemaname IN ('public','trading','user_management') LOOP
    EXECUTE 'ALTER SEQUENCE ' || quote_ident(r.schemaname) || '.' || quote_ident(r.sequencename) || ' OWNER TO algo_dev';
  END LOOP;
  FOR r IN SELECT viewname FROM pg_views
           WHERE schemaname IN ('public','trading','user_management') LOOP
    EXECUTE 'ALTER VIEW ' || quote_ident(r.viewname) || ' OWNER TO algo_dev';
  END LOOP;
  FOR r IN SELECT schemaname, matviewname FROM pg_matviews
           WHERE schemaname IN ('public','trading','user_management') LOOP
    EXECUTE 'ALTER MATERIALIZED VIEW ' || quote_ident(r.schemaname) || '.' || quote_ident(r.matviewname) || ' OWNER TO algo_dev';
  END LOOP;
  FOR r IN SELECT nspname FROM pg_namespace
           WHERE nspname IN ('public','trading','user_management') LOOP
    EXECUTE 'ALTER SCHEMA ' || quote_ident(r.nspname) || ' OWNER TO algo_dev';
  END LOOP;
END\$\$;
"
```

Fix app-specific functions (skip TimescaleDB internals):

```bash
sudo -u postgres psql -d algostaking_dev -c "
ALTER FUNCTION public.update_updated_at_column() OWNER TO algo_dev;
ALTER FUNCTION public.refresh_signal_performance() OWNER TO algo_dev;
"
```

Grant default privileges for future objects:

```bash
sudo -u postgres psql -d algostaking_dev -c "
GRANT ALL ON SCHEMA public TO algo_dev;
GRANT ALL ON SCHEMA trading TO algo_dev;
GRANT ALL ON SCHEMA user_management TO algo_dev;
GRANT ALL ON ALL TABLES IN SCHEMA public TO algo_dev;
GRANT ALL ON ALL TABLES IN SCHEMA trading TO algo_dev;
GRANT ALL ON ALL TABLES IN SCHEMA user_management TO algo_dev;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO algo_dev;
GRANT ALL ON ALL SEQUENCES IN SCHEMA trading TO algo_dev;
GRANT ALL ON ALL SEQUENCES IN SCHEMA user_management TO algo_dev;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO algo_dev;
ALTER DEFAULT PRIVILEGES IN SCHEMA trading GRANT ALL ON TABLES TO algo_dev;
ALTER DEFAULT PRIVILEGES IN SCHEMA user_management GRANT ALL ON TABLES TO algo_dev;
"
```

### 9. Start All Services

```bash
algostaking start dev
```

Wait ~10 seconds for all services to initialize.

### 10. Publish Configuration

The configuration service seeds reference data (venues, assets, bar types, subscriptions):

```bash
sleep 10
curl -s -X POST http://localhost:9007/publish/all
```

Expected response: `{"registry":{"assets":51,"venues":12,...},"subscriptions":{"market_subscriptions":213,...}}`

### 11. Final Restart

Services need a restart to consume the newly-published configuration:

```bash
algostaking restart dev
sleep 10
curl -s -X POST http://localhost:9007/publish/all
```

### 12. Verify

```bash
algostaking health dev
```

All 12 services should be `active`. Check pipeline flow after ~30 seconds:

```bash
curl -s localhost:9000/metrics | grep ticks_received     # ingestion: should be >0
curl -s localhost:9001/metrics | grep bars_published      # aggregation: should be >0
curl -s localhost:9004/metrics | grep predictions_published  # prediction: should be >0
curl -s localhost:9008/metrics | grep signals_published      # signal: should be >0
curl -s localhost:9012/metrics | grep targets_published      # pms: should be >0
curl -s localhost:9013/metrics | grep orders_created         # oms: should be >0
curl -s localhost:9009/metrics | grep fills_published        # ems: should be >0
```

## Gotchas

- **Persistence fails on `trades` table**: If you copied `trades` from prod, it may have wrong schema. Drop it and let persistence recreate: `sudo -u postgres psql -d algostaking_dev -c "DROP TABLE IF EXISTS trades CASCADE;"`
- **API fails with "must be owner"**: Ownership fix in step 8 was incomplete. Run the ownership block again.
- **TimescaleDB compression errors**: Expected on Apache license. Ignore.
- **NEVER run this on prod**: This procedure is for `algostaking_dev` only.
