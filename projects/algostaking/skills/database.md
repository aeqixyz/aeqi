# TimescaleDB / PostgreSQL

## Connection Info

| Environment | Database | User | Host |
|-------------|----------|------|------|
| DEV | algostaking_dev | algo_dev | localhost:5432 |
| PROD | algostaking_prod | algo_prod | localhost:5432 |

## Connection Strings
```
DEV:  postgresql://algo_dev:PASSWORD@localhost:5432/algostaking_dev
PROD: postgresql://algo_prod:PASSWORD@localhost:5432/algostaking_prod
```

Passwords stored in `/etc/algostaking/backend-{dev,prod}.env`

## Common Operations

### Connect
```bash
# As app user
psql -U algo_dev -d algostaking_dev

# As superuser
sudo -u postgres psql algostaking_dev
```

### Check TimescaleDB
```sql
\dx                          -- List extensions
SELECT * FROM timescaledb_information.hypertables;
```

### Useful Queries
```sql
-- Check table sizes
SELECT hypertable_name, pg_size_pretty(hypertable_size(format('%I.%I', hypertable_schema, hypertable_name)::regclass))
FROM timescaledb_information.hypertables;

-- Check chunks
SELECT * FROM timescaledb_information.chunks;

-- Check continuous aggregates
SELECT * FROM timescaledb_information.continuous_aggregates;
```

## Schema

Location: `/home/claudedev/algostaking-backend/infrastructure/schema/`

### CRITICAL RULE: Schema = Source of Truth

**When modifying database structure, you MUST update the schema files.**

```
infrastructure/schema/
├── trading_tables.sql      # PMS/OMS/EMS tables
├── registry_tables.sql     # Assets, venues, bar types
├── subscription_tables.sql # Market/bar subscriptions
└── signal_tables.sql       # Signals, predictions
```

**The rule:**
- Schema files must reflect the CURRENT production state
- A fresh `psql -f *.sql` must create a working database
- No separate migrations - just update the schema files directly
- This ensures: purge DB → apply schema → everything works

**When you ALTER a table:**
1. Make the change in production/dev via SQL
2. **IMMEDIATELY** update the corresponding `infrastructure/schema/*.sql` file
3. Verify: fresh setup would produce identical schema

### Fresh Database Setup

**IMPORTANT**: Always create schema as the app user (algo_dev), not postgres!

```bash
# 1. Drop and recreate database (as postgres)
sudo -u postgres psql -c "DROP DATABASE IF EXISTS algostaking_dev;"
sudo -u postgres psql -c "CREATE DATABASE algostaking_dev OWNER algo_dev;"
sudo -u postgres psql algostaking_dev -c "CREATE EXTENSION IF NOT EXISTS timescaledb;"

# 2. Apply schema files AS THE APP USER
export PGPASSWORD=$(grep DB_PASSWORD /etc/algostaking/backend-dev.env | cut -d= -f2)
psql -U algo_dev -d algostaking_dev -f infrastructure/schema/trading_tables.sql
psql -U algo_dev -d algostaking_dev -f infrastructure/schema/registry_tables.sql
psql -U algo_dev -d algostaking_dev -f infrastructure/schema/subscription_tables.sql
psql -U algo_dev -d algostaking_dev -f infrastructure/schema/signal_tables.sql

# 3. Restart services (persistence will create hypertables)
algostaking restart dev

# 4. Publish initial config data
curl -X POST http://127.0.0.1:9007/publish/all
```

### Fix Ownership Issues

If tables were created by postgres instead of algo_dev:

```bash
sudo -u postgres psql algostaking_dev -c "
DO \$\$ DECLARE r RECORD; BEGIN
    FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname='public' AND tableowner='postgres') LOOP
        EXECUTE 'ALTER TABLE ' || quote_ident(r.tablename) || ' OWNER TO algo_dev';
    END LOOP;
    FOR r IN (SELECT matviewname FROM pg_matviews WHERE schemaname='public' AND matviewowner='postgres') LOOP
        EXECUTE 'ALTER MATERIALIZED VIEW ' || quote_ident(r.matviewname) || ' OWNER TO algo_dev';
    END LOOP;
    FOR r IN (SELECT viewname FROM pg_views WHERE schemaname='public' AND viewowner='postgres') LOOP
        EXECUTE 'ALTER VIEW ' || quote_ident(r.viewname) || ' OWNER TO algo_dev';
    END LOOP;
END \$\$;
"
```

## Backup & Restore

```bash
# Backup
pg_dump -U algo_dev algostaking_dev > backup.sql

# Restore
psql -U algo_dev algostaking_dev < backup.sql
```

## Troubleshooting

### Permission Issues
```sql
-- Grant all on schema
GRANT ALL ON ALL TABLES IN SCHEMA public TO algo_dev;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO algo_dev;
GRANT ALL ON ALL FUNCTIONS IN SCHEMA public TO algo_dev;

-- Transfer ownership
ALTER TABLE tablename OWNER TO algo_dev;
```

### Check Connections
```sql
SELECT * FROM pg_stat_activity WHERE datname = 'algostaking_dev';
```
