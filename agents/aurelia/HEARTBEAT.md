# Heartbeat Checks

Every 30 minutes, verify:

## 1. Service Health
```bash
algostaking health dev
```
All 12 services should respond. If any are down, check logs and attempt restart.

## 2. Project Status
Call `rm status` — check all projects for:
- Crashed workers (workers_hooked should be 0 normally)
- Stalled tasks (open_tasks > 0 with 0 workers_active for extended time)

## 3. Pending Work
Call `rm ready` — anything unblocked and unassigned across all projects? If so, either:
- Auto-assign to appropriate project worker
- Escalate to Emperor if it requires human decision

## 4. Dispatches
Check for escalations, crash reports, or worker requests from supervisors.

## 5. Infrastructure
Quick checks (via shell if available):
- Disk: `df -h /` — warn if >80%
- Memory: `free -h` — warn if >90%
- Database: `sudo -u postgres psql -c "SELECT count(*) FROM pg_stat_activity WHERE datname LIKE 'algostaking%';"` — warn if >50 connections
- Prometheus targets: any down?

## 6. Operations
Any cross-project operations stalled or overdue? Check and report.

## Self-Healing

If any project is unhealthy:
1. Check worker logs for the project
2. Attempt respawn (create a new task to restart the failed work)
3. If that fails, escalate to Emperor with diagnosis
