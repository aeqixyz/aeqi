# Troubleshooting Guide

## Service Won't Start

```bash
# 1. Check status and recent logs
sudo systemctl status algostaking-dev-{service}
sudo journalctl -u algostaking-dev-{service} --no-pager -n 100

# 2. Common errors:
# - "Address already in use" → Port conflict, check with: ss -tlnp | grep PORT
# - "failed to lookup address" → ZMQ using Docker hostname, fix config to use 127.0.0.1
# - "config not found" → Check WorkingDirectory in service file
# - "permission denied" → Check file ownership and service User=
```

## Service Running But Not Working

```bash
# Check if metrics endpoint responds
curl http://localhost:PORT/metrics | head -10

# Check ZMQ connections in logs
sudo journalctl -u algostaking-dev-{service} | grep -i "connect\|bind\|zmq"

# Check database connectivity (for services that use DB)
sudo journalctl -u algostaking-dev-{service} | grep -i "database\|postgres\|sql"
```

## Prometheus Not Scraping

```bash
# Check target status
curl -s http://localhost:9090/api/v1/targets | python3 -c "
import json,sys
for t in json.load(sys.stdin)['data']['activeTargets']:
    if t['health'] != 'up':
        print(f\"{t['labels']['job']}: {t['lastError']}\")"

# Common issues:
# - Wrong port in prometheus config
# - Service not exposing /metrics endpoint
# - Firewall blocking
```

## Grafana Dashboard Empty

1. **Check datasource**: Settings → Data Sources → Prometheus → Test
2. **Check job names**: Dashboard queries must match Prometheus job names
3. **Check time range**: Ensure data exists in selected time range
4. **Check variables**: Some dashboards filter by `env`, `execution` etc.

```bash
# Verify data exists in Prometheus
curl -s "http://localhost:9090/api/v1/query?query=up"
```

## Database Issues

```bash
# Check PostgreSQL running
sudo systemctl status postgresql

# Check connections
sudo -u postgres psql -c "SELECT * FROM pg_stat_activity WHERE datname LIKE 'algostaking%';"

# Check disk space
df -h /var/lib/postgresql
```

## Nginx/SSL Issues

```bash
# Test config
sudo nginx -t

# Check error logs
sudo tail -f /var/log/nginx/error.log

# Check certificate
sudo certbot certificates

# Renew certificate
sudo certbot renew
```

## High Memory/CPU

```bash
# Check service resource usage
systemctl status algostaking-dev-* | grep -E "Memory|CPU"

# Top consumers
ps aux --sort=-%mem | head -20

# Check for runaway processes
top -b -n 1 | head -20
```

## Network Issues

```bash
# Check listening ports
ss -tlnp | grep -E ":(80|443|8081|8082|9000)"

# Check ZMQ ports
ss -tlnp | grep -E ":(555|556|557)"

# DNS resolution
host api.algostaking.com
```

## Quick Health Check Script

```bash
#!/bin/bash
echo "=== Services ==="
algostaking health dev

echo -e "\n=== Prometheus Targets ==="
curl -s http://localhost:9090/api/v1/targets | python3 -c "
import json,sys
data=json.load(sys.stdin)
up = len([t for t in data['data']['activeTargets'] if t['health']=='up'])
total = len(data['data']['activeTargets'])
print(f'{up}/{total} targets UP')"

echo -e "\n=== Grafana ==="
curl -s http://localhost:3000/api/health | python3 -c "import json,sys; print(json.load(sys.stdin))"

echo -e "\n=== Database ==="
sudo -u postgres psql -c "SELECT datname, numbackends FROM pg_stat_database WHERE datname LIKE 'algostaking%';" 2>/dev/null || echo "Check failed"
```
