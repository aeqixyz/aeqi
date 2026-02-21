# Heartbeat Checks

Every 30 minutes, verify:

1. **Rig Health**: All registered rigs have active witnesses, no crashed workers
2. **Pending Work**: Check `sg ready` across all rigs — anything unblocked and unassigned?
3. **Mail**: Read incoming mail from witnesses — any escalations or crash reports?
4. **Convoys**: Any cross-rig convoys stalled or overdue?
5. **Cron**: Any failed cron jobs since last heartbeat?

If any rig is unhealthy, attempt self-healing first (respawn workers). If that fails, escalate to Emperor.
