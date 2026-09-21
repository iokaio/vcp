# P6-05 / M3 — Read-only action forecasts

`/optimize forecasts` and `vcp optimize forecasts` inspect bounded action episodes
rebuilt from authorized canonical history. They make no provider request, change
no policy, and save no state. Existing observed totals remain separate.

An eligible episode needs a retained start, causally ordered actions and a
retained terminal task outcome. Paused, blocked, open and window-truncated work
is not relabeled completed or failed. Gaps, ambiguous concurrency and unsupported
lineage are visible exclusions. Cohorts use historical model/endpoint, task-class
and policy identities; a current registry cannot relabel an older episode.

The projection counts each admitted attempt once and attaches its exact final
charge once. Check-only verification observations incur no additional provider
charge. Unknown usage and liabilities never become zero-cost visits. Forecasts
are model expectations for supported cohorts, not actual accounting totals,
spending limits, causal improvements or completion-within-budget probabilities.

The observed-support absorbing chain reports sample denominators, transition
counts, expected visits and terminal outcomes. Unsupported, sparse or invalid
models abstain. Inspection remains unqualified; support counts and explicit
limitations accompany estimates rather than implying calibrated confidence.

This increment provides the projection and read-only inspection. Saved
source-linked forecast artifacts, matched drift comparisons and compaction
diagnostics follow before M3 is complete. M4 supplies qualification or a recorded
rejection without enabling unqualified consumers.

Bounds are 512 complete episodes, 64 cohorts and 4096 visits, with at least
20 complete episodes and five observations per transient row before estimates.
Zero observed transition support remains zero; no synthetic successful outcome
is added. Unknown charges or liabilities suppress expected costs.

Verification: two arithmetic unit tests, three canonical integration tests
(each covering Files and SQLite), and two CLI tests passed. Integration checks
include hand-calculated visits/outcomes/costs, late charge correction, unknown
liabilities, access restrictions, reopen, partial windows, revision gaps and
logical purge. Formatting and diff checks passed. The fast suite passed all
nine cases (`91890e55-2f78-4ad2-8e65-e35adfd38f50`). These are deterministic
contract checks, not held-out predictive qualification.
