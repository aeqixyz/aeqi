# Prometheus & Grafana Monitoring

## Architecture

Separate instances per environment for isolation.

| Component | DEV | PROD |
|-----------|-----|------|
| Prometheus | localhost:9090 | localhost:9091 |
| Grafana | localhost:3000 | localhost:3002 |
| URL | https://dev.grafana.algostaking.com | https://grafana.algostaking.com |

## Prometheus

### Config Files
- DEV: `/etc/prometheus/prometheus-dev.yml`
- PROD: `/etc/prometheus/prometheus-prod.yml`

### Service Management
```bash
sudo systemctl status prometheus-dev
sudo systemctl restart prometheus-dev
sudo journalctl -u prometheus-dev -f
```

### Check Targets
```bash
# All targets status
curl -s http://localhost:9090/api/v1/targets | python3 -c "
import json,sys
data=json.load(sys.stdin)
for t in data['data']['activeTargets']:
    print(f\"{t['labels']['job']:25} {t['health']:6} {t['labels']['instance']}\")"

# Or via web UI
# http://localhost:9090/targets
```

### Job Names (must match dashboards)
| Job Name | Service | Port |
|----------|---------|------|
| data_ingestion | Ingestion | 9000 |
| data_aggregation | Aggregation | 9001 |
| data_persistence | Persistence | 9002 |
| strategy_feature | Feature | 9003 |
| strategy_prediction | Prediction | 9004 |
| strategy_signal | Signal | 9008 |
| configuration | Configuration | 9007 |
| gateway_api | API | 8082 |
| gateway_stream | Stream | 8081 |
| pms | PMS | 9012 |
| oms | OMS | 9013 |
| ems | EMS | 9009 |

### Query Examples
```bash
# Check if metric exists
curl -s "http://localhost:9090/api/v1/query?query=run_info"

# Check specific service metrics
curl -s "http://localhost:9090/api/v1/query?query=up{job='data_ingestion'}"
```

## Grafana

### Service Management
```bash
sudo systemctl status grafana-dev
sudo systemctl restart grafana-dev
sudo journalctl -u grafana-dev -f
```

### Credentials
- Default: admin / admin (change on first login)
- Reset password: `sudo grafana-cli admin reset-admin-password newpassword`

### Provisioning
- Datasources: `/etc/grafana/provisioning/datasources/`
- Dashboards: `/etc/grafana/provisioning/dashboards/`
- Dashboard JSON: `/var/lib/grafana/dashboards/`

### Datasource Config
Datasource UID must be `prometheus` (dashboards expect this).

```yaml
# /etc/grafana/provisioning/datasources/prometheus.yaml
apiVersion: 1
datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://localhost:9090
    uid: prometheus
    isDefault: true
```

### Dashboard Folders
- Data Services: ingestion, aggregation, persistence
- Strategy Services: feature, prediction, signal
- Gateway Services: api, stream, configuration
- Trading Services: pms, oms, ems

### Common Issues

**Datasource not found:**
- Check UID matches `prometheus` in provisioning
- Restart Grafana after config changes

**Dashboard shows no data:**
- Check Prometheus job names match dashboard queries
- Verify time range includes data
- Check `env` variable filter if dashboards use it
