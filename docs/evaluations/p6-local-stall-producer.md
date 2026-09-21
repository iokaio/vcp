# P6-03 / M2 — Frozen local statistical producer

The local producer separates explicit offline fitting from inference. It builds
a bounded second-order categorical model over retained failed-verification
symbols, then evaluates new observations against those frozen counts. Inference
does not update model parameters. Training and inference windows cannot overlap.

The fit is scoped to one task and its input/steering identity. Verification
observations do not identify a provider endpoint, so the producer makes no
endpoint-cohort claim. Unknown symbols, insufficient samples, unavailable
observations and invalidated sources cannot produce supported advice. Exact-cycle
facts are included separately for comparison.

The statistical result reports conditional row support, smoothed transition
likelihood and normalized entropy. Repeated-strategy suspicion is an experimental
threshold on these statistics, not a calibrated probability of a stall. Neither
the exact detector nor the statistical signal can choose a model, skip a check,
grant authority or change an escalation.

Every fit identifies its algorithm, source evidence, frozen window/cutoff,
authority, deletion epoch, policy, catalog and symbol alphabet. Appending new
observations leaves the frozen model usable; deleting or changing its training
evidence invalidates it. Current access checks still apply after reopen.

This increment supplies the producer library. The subsequent
[local shadow runtime](p6-local-shadow-runtime.md) adds owner selection,
asynchronous inference and retained receipts. Qualification and any behavioral
influence remain M4.
No HMM is introduced without held-out evidence that simpler signals are
insufficient.

Verification: three model tests, one lifecycle symbol/reset test and one
integration test covering both stores passed. Integration exercises frozen
parameters, forged-fit rejection at installation, later observations, unchanged
state, access restrictions, reopen and source purge. Formatting and diff checks
passed. The nine-case fast suite passed
(`8e1aefc0-6f39-4493-a2db-e25c3047a360`). No provider calls were made.
