# ADR-032 — Rebuildable unqualified Markov fit artifacts

Status: selected for the P6-02/M1 fit-provenance increment. This defines a local
candidate artifact and abstention boundary. It does not qualify, persist or serve
a forecast.

## Evidence and problem

ADR-029 provides retained task-state evidence and the bounded kernels can fit a
validated chain, but `vcp_models::markov::Chain` is intentionally not a portable
artifact. A consumer cannot assess a matrix without its exact retained source,
authority/deletion boundary, alphabet, cohort, algorithm, prior and sample gate.
Sparse or nonabsorbing history must remain an explicit abstention rather than an
empty or zero-probability model.

## Decision

Add `routing_state::fits::fit`, a read-only rebuild from one authorized
`canonical-task-state/1` evidence view. It emits version-one `Artifact` with:

- workspace, authority, deletion revision, source ID/digest, cutoff, window and
  scoped task set, plus gap/censoring/pruned-source coverage;
- source alphabet, selected observed-state alphabet, absorbing flags, integer
  counts and raw row sample sizes;
- the fixed `first-order-observed-support/1` algorithm, integer basis-point prior,
  minimum-sample gate and `raw-row-minimum-samples/1` uncertainty method;
- explicit task-state feature/cohort, unavailable task-class/endpoint dimensions,
  and current policy/catalog identities when configured; and
- either a finite validated probability matrix or a closed abstention reason.

The candidate alphabet follows the fixed task-state order but includes only
states on retained connected edges. This avoids creating unobserved outcome
states with smoothing. It is explicitly a narrower observed cohort; absent
terminal outcomes remain unavailable. Kernel smoothing applies only to observed
legal edges. Task lifecycle rules define legality, completed/failed/cancelled are
absorbing, and blocked/paused/waiting remain transient.

Empty, terminal-free, terminal-only, sparse, forbidden, nonabsorbing, invalid or
numerically unsafe inputs abstain. Configuration parameters are bounded and part
of artifact identity. The artifact always has no qualification ID and
`serving_qualified = false`.

## Retention and lifecycle

The artifact is rebuilt and never written to the canonical store. Its identity
hashes the complete result and source digest. Changed source history, window,
scope, authority, deletion revision, policy, catalog or parameters produces a
different artifact or denial. No cached result can outlive source pruning.

Later persistence is limited to qualified metadata or an exact value actually
consumed by a decision. Replay must use that recorded consumed value rather than
refitting. This decision supplies neither persistence nor consumption.

## Consequences and remaining work

M1 has a provenance-complete first-order candidate and deterministic abstention.
It still needs held-out first/second-order comparison with complexity penalties,
multi-step checks, reward mapping/uncertainty qualification, and consumed-value
replay. M2/M3 remain disabled.

Verification commands and results are recorded in the
[increment evidence](../evaluations/p6-fit-provenance.md).
