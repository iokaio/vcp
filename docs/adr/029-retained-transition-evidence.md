# ADR-029 — Retained task-transition evidence

Status: selected for the first P6-02/M1 increment. This decision defines a local
read-only observation boundary; it selects no statistical model, routing default
or fitted artifact format. Numerical/reward and consumer qualification remain
separate M1/M4 work in the [Markov sequence](../plan/21-markov-integration.md).

## Evidence and problem

The existing optimizer counts current canonical task/attempt rows for tasks with
events in a selected window. It explicitly does not reconstruct historical task
state. Treating those counts as Markov transitions would invent paths and permit
future outcomes to enter an earlier window.

Engine events already carry version-one `facts` with complete typed task
snapshots, revisions and their causing event identity. Some other retained event
paths, including restore pause, record only a reason. A missing snapshot cannot
be reconstructed from the current row or adjacency in wall-clock timestamps.

## Decision

Add `routing_state::transitions::observe`, reached through the canonical routing
control boundary and `vcp optimize transitions` / `/optimize transitions`. Read a
single canonical retained view, filter by current workspace/task authorization
and inclusive-from/exclusive-until timestamps, and retain append order.

For each authorized task, decode version-one task facts, check scope, revision,
source identity and shape, then link only adjacent task revisions. State changes
become counts over the closed `canonical-task-state/1` alphabet. Same-state
steering/fingerprint revisions preserve continuity but are not additional state
visits. Separate task traces never form cross-task edges. Blocked/paused are
resumable; only completed/failed/cancelled are terminal.

Missing, redacted, invalid or discontinuous evidence breaks a chain and is
reported explicitly. Open traces and truncated starts/ends are censored. Do not
fill a gap from a current terminal row. Bounds are 100,000 stored rows/events,
100,000 examined facts, 4,096 permitted observations/gaps and 512 KiB of encoded
evidence (leaving room in the CLI's 1 MiB response envelope); exceeding them
returns an error requesting a narrower window, never silently truncated counts.

The result contains typed states, revisions, permitted source event IDs, source
window, current authority/deletion revisions, canonical cutoff, limitations and a
digest of this normalized evidence. It includes no objective, source prose or
provider reasoning. No result is saved in the store and no background task,
reservation or gateway request is created. Reads remain available with no write
permission, routing configuration or model budget.

## Retention, replay and compatibility

Current task redaction or a logical purge decision excludes its historical
snapshots before physical cleanup. Event redaction/purge breaks continuity;
retention dependency closure can exclude the entire task when its snapshot is
selected. Scoped access filters before counts. Every invocation rebuilds
from retained evidence; no new cache, aggregate, migration or portable asset can
retain pruned work. Reopen/restore use the same retained source format. Existing
optimizer reports keep their schema and current-state meaning.

The same retained view, access and window produce the same digest and observations;
later deletion or authority changes create a different view. This is evidence
inspection, not a consumed probabilistic decision receipt. Persisting fitted
models or consuming their values requires the remaining M1 retention/replay
design and per-purpose qualification; this API does not authorize either.

## Alternatives and limits

Rejected reconstructing from current rows, sorting solely by time, bridging
missing revisions and persisting an aggregate before its retention contract.
Task states alone lack action/failure signatures, attempt counters, endpoint
cohorts and rewards. The result is not a sufficient forecasting state and must
not feed routing as a probability or expected cost. Those extensions need a new
alphabet/version with their owning M1 evidence, rather than silently changing
these counts. ADR-007/017/020 remain the authority for later consumption.

## Verification

The focused `transition_evidence` tests exercise both Files and SQLite: interleaved
tasks, clock rollback, blocked/resume, same-state revisions, missing snapshots,
historical cutoffs, scoped/denied reads, unchanged canonical state, reopen and
source purge. CLI tests verify window mapping and the single read-only request.
Actual commands and outcomes are recorded in the
[increment evidence](../evaluations/p6-transition-evidence.md).
