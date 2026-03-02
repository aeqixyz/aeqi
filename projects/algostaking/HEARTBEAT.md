# Heartbeat Checks

Every 30 minutes, verify:

1. **Service Health**: All 12 services running (`systemctl status algostaking-dev-*`)
2. **Pipeline Flow**: EMS receiving ticks, PMS processing signals, OMS executing orders
3. **Metrics**: Check Prometheus for anomalies (latency spikes, error rates)
4. **Database**: PostgreSQL responding, no stuck queries
5. **Alerts**: Check AlertManager for firing alerts

If any check fails, create a task and escalate.
