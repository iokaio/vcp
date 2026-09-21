# ADR-030 — Causal action, effect and attempt observations

Status: selected for the P6-02/M1 action-evidence increment. This extends the
retained source boundary from ADR-029; it does not select a statistical model,
persist a fit or authorize a routing consumer.

## Evidence and problem

Canonical version-one events already retain typed turn, effect and verification facts.
Accounting events retain complete attempt snapshots and exact provider/model,
role, retry predecessor, policy and phase. Routing decision records can add task
class for the same admitted attempt. Those records have different causal units:
turn revisions form one sequence, tool/effect revisions form another, attempt
phases form another, retry predecessors form lineage, and verification checks are
point observations. Sorting all events
into one task sequence would invent transitions between concurrent or independent
work.

Verification snapshots bind steering and repository/buffer/environment
fingerprints, but older events do not retain the accepted task revision. Without
that revision, two similar diagnostic strings cannot prove the same failure on
the same input.

## Decision

Add `routing_state::observations::observe`, exposed read-only as
`vcp optimize observations` and `/optimize observations`. Rebuild a single
authorized retained view at an inclusive-from/exclusive-until window and canonical
watermark. Preserve append order; timestamps select a window but never order
causal observations.

Use the closed `canonical-action-observation/1` alphabet. Emit separate turn,
effect and attempt traces with consecutive revisions, typed states/phases, source event IDs,
watermarks and censoring. Never link separate turns or attempts. Record exact
Effect traces expose only the canonical operation digest, typed lifecycle,
execution presence, exit code and changed-artifact count; they include no command
arguments, output or reason. Record exact attempt predecessor lineage and expose retry depth only when its retained prefix
is complete. Task decomposition depth requires an authorized retained parent chain.

Attempt cohorts contain the closed request role, exact admitted model/endpoint,
authority policy, root-task flag and available task class. Accept task class only
from a retained `vcp_routing_decision_v1` whose workspace/task/root/attempt,
selected endpoint/model, request digest and record references match the attempt.
Counters for prior attempts and failed checks are available only from an
untruncated retained prefix. Missing inputs remain `None`.

For new `VerificationRecorded` events, add optional
`observed_task_revision` alongside existing facts. Old version-one events remain
readable with no migration and yield an unavailable task revision. Check identity
is a digest of a bounded whitespace/case-normalized specification. A failure
signature digests that identity, observed task revision, steering, input
fingerprint, exit code and normalized diagnostic digest. Raw specifications,
diagnostics, reasons and objectives are never returned. Failed checks without a
valid task revision or bounded diagnostic have no signature; they are not treated
as a distinct exact failure.

## Validation, bounds and retention

Validate scope, session, identity, revision, current retained record, event kind,
turn/effect transition legality, attempt phase, immutable attempt cohort and event
metadata. Missing, malformed, duplicate, discontinuous, redacted or purged inputs
create explicit gaps or exclude the affected record. Terminal turn states are
completed/failed/cancelled; paused, blocked, waiting and budget-exhausted remain
resumable/censored.

Bound the canonical store scan and examined facts to 100,000 each, emitted
observations/gaps to 4,096, lineage/decomposition depth to 256, normalized
diagnostic input to 64 KiB and encoded evidence to 512 KiB. Exceeding a bound
fails visibly and requests a narrower window/scope.

Every read checks current authority, task scope, logical purge decisions and
physical redaction. The projection is rebuilt and never stored, so it cannot
outlive source deletion. Retention protection still prevents purging active turns
or unsettled accounting; the inspector does not weaken that rule. Both stores,
reopen and old-event compatibility use the same source contracts.

## Consequences and remaining work

The result is structured input for later counting, fitting and exact-cycle
analysis. It is not itself a Markov state, fitted probability, reward, failure
cause, quality score or routing recommendation. Attempt cost attribution and
unknown liability remain the next M1 increment. Fitted-artifact provenance,
uncertainty, retention invalidation and consumed-value replay remain subsequent
M1 work. ADR-007/017/020 continue to govern any later consumption.

Verification commands and results are recorded in the
[increment evidence](../evaluations/p6-action-evidence.md).
