# P6-03 / M2 — Exact repetition evidence

The read-only `/optimize cycles` command (also available through the offline
optimizer) rebuilds exact failed-verification patterns from M1's authorized
canonical observations. It issues no provider request and writes no state.

The versioned detector considers periods of one to eight complete verification
symbols and requires at least three repetitions. Each symbol includes the ordered
check and failure identities. Results identify the original verifications/events,
input fingerprint, task revision and steering. Source digest, window, cutoff,
authority, deletion epoch, policy and catalog accompany the result.

Successful, unavailable or incomplete checks and changes to task, steering,
revision, input or unresolved-work counts break continuity. Missing evidence
causes conservative exclusion. The projection is bounded to 4096 observations
and checks; it neither joins partial cycles nor crosses task boundaries.
Repeated diagnostics can describe flaky checks or an environment failure, so an
exact match is an observed fact, not a stall diagnosis or escalation permission.
The first mapping covers verification observations only.

Rebuilding uses current access and retention checks, including logical purge
before cleanup. Nothing is cached or restored from an older projection. Required
checks, review, counters, model selection and budgets are unchanged.

The next M2 increment compares a separately identified local statistical shadow
signal against these facts. Outcome qualification remains M4/P6-04.

Verification: three detector unit tests, two CLI tests and one integration test
covering both Files and SQLite passed. The integration checks canonical engine
observations, unchanged state, access restrictions, reopen and logical purge.
Formatting and diff checks passed. The fast suite passed all nine cases
(`ae9615ad-683d-4e16-97db-2462854a0e99`). No live provider calls were made.
