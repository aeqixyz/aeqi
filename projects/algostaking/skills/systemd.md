# Systemd Service Management

## AlgoStaking Services

All services follow pattern: `algostaking-{env}-{service}`

### Service List
| Service | DEV Unit | Metrics Port |
|---------|----------|--------------|
| ingestion | algostaking-dev-ingestion | 9000 |
| aggregation | algostaking-dev-aggregation | 9001 |
| persistence | algostaking-dev-persistence | 9002 |
| feature | algostaking-dev-feature | 9003 |
| prediction | algostaking-dev-prediction | 9004 |
| configuration | algostaking-dev-configuration | 9007 |
| signal | algostaking-dev-signal | 9008 |
| ems | algostaking-dev-ems | 9009 |
| api | algostaking-dev-api | 8082 (main) |
| stream | algostaking-dev-stream | 8081 (main) |
| pms | algostaking-dev-pms | 9012 |
| oms | algostaking-dev-oms | 9013 |

### Management Script
```bash
# Preferred method - uses /usr/local/bin/algostaking
algostaking start dev      # Start all dev services
algostaking stop dev       # Stop all dev services
algostaking restart dev    # Restart all dev services
algostaking status dev     # Show service status
algostaking health dev     # Health check all services
algostaking logs dev api   # View logs for specific service

# Direct systemctl (if needed)
sudo systemctl status algostaking-dev-ingestion
sudo journalctl -u algostaking-dev-ingestion -f
```

### Service Files Location
- `/etc/systemd/system/algostaking-dev-*.service`
- `/etc/systemd/system/algostaking-prod-*.service`

### Environment Files
- DEV: `/etc/algostaking/backend-dev.env`
- PROD: `/etc/algostaking/backend-prod.env`

### Config Files
- Service configs: `/home/claudedev/algostaking-backend/config/{dev,prod}/*.yaml`
- Each service reads `config/service.yaml` from its working directory

### Common Issues

**Service won't start:**
```bash
sudo journalctl -u algostaking-dev-{service} --no-pager -n 50
```

**ZMQ connection failures:**
- Check endpoints use `127.0.0.1` not Docker hostnames
- Check ports in config match what's expected

**Config not loading:**
- Verify WorkingDirectory in service file
- Check config/{dev,prod}/*.yaml exists and is symlinked
