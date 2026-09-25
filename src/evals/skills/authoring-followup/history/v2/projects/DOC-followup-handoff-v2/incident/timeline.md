# Relay queue incident RQ-28
All times are UTC on 2026-08-12. This is a synthetic incident record.

- 13:40: Queue age crossed the 12-minute warning; intake was still accepting jobs.
- 13:47: Outgoing operator Mina paused intake using the documented procedure.
- 13:52: Mina saved a diagnostic snapshot as ticket attachment RQ-28/snapshot-1352. The attachment is not in this workspace.
- 14:02: Mina inspected the last 30 worker log entries. They contained three R42 retry messages and no R90 storage-integrity messages.
- 14:08: With incident lead approval, Mina restarted the worker once. This used the incident's sole restart allowance.
- 14:14: Queue count was 34 and oldest queued job age was 9 minutes. Intake remained paused.
- 14:20: Queue count was 18 and oldest queued job age was 6 minutes. This is the latest observation, not a recovery declaration.

The retry cause has not been established. A connection timeout is a hypothesis, not a diagnosis.
