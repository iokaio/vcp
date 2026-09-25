# Relay bounded recovery procedure
The incoming operator first reviews the incident record and obtains a fresh read-only queue count, oldest-job age and last 30 worker log entries.

Keep intake paused during observation. Resume is permitted only after two observations at least five minutes apart both show queue count at most 10 and oldest-job age below 3 minutes, with no R90 entry. Incident lead approval is required before resuming intake.

At most one worker restart may be approved per incident. A used allowance does not renew with a shift change. Any R90 entry, or a queue count increase across two consecutive observations, means stop recovery changes and escalate to the incident lead. Continue recording observations while awaiting direction if safe.

Do not purge queued jobs or replay completed jobs as part of this procedure.
