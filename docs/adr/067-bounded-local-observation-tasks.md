# ADR-067 — Bounded local observation tasks

Status: selected for P10-03; qualification evidence is recorded separately.

## Decision

Ship an optional, disabled-by-default exact verification repetition observer.
It offers grouped references to repeated failed checks on unchanged input.
It makes no statistical regime estimate and changes no task state, permission,
budget, memory acceptance or verification result. M2/M4's unqualified statistical
filter is not a runtime prerequisite; the M10 regime candidate remains deferred.

The canonical owner schedules bounded observation work at existing lifecycle
boundaries. There is no independent background loop. Versioned observer state,
input identity, durable cursor, deduplication, attempt and proposal live in a
scoped canonical projection. This is a host-owned local observation task under
the root, distinct from a delegated model child. Its work has no executable
capabilities or provider path. Do not construct a fictitious positive model
allocation merely to fit the delegated `ChildSpec` contract.

Retain both input-level deduplication and proposal-level deduplication. A newly
retained verification may require analysis, but an advancing watermark or fourth
identical failure must not create a second notice for the same pattern/revision.
Fresh verification does not emit a fingerprint change when the fingerprint is
unchanged. This preserves the relevant task revision across repeated native
checks; changed input still advances it, and stale runs cannot update it.
Debounce before compute admission. Bound queue, outstanding attempts, frequency,
steps, deadline and retained state. Expose zero billable cost alongside actual
local work; zero dollars does not imply unbounded free work.

Prepare only currently authorized retained context, compute from immutable input,
then validate current owner admission, parent state and source revisions before
publishing current advice. A correction, pause or owner loss invalidates pending
publication. Late results remain historical. Recovery never blindly reruns a
claimed attempt: explicit resume and relevance validation are required.

Contributing verification records participate in the normal retention closure.
Forgetting them redacts derived observer state to a content-free identity and
leaves this root's observer unavailable. Reconfiguration cannot turn that
tombstone into a fresh budget. This conservative behavior avoids retaining
derived private evidence or silently resetting no-replay history.

Advice remains data for the controller/user. It is not an executable action or a
source of authority. Any subsequent action must use normal command validation
and fresh permissions. A future model-backed observer requires a separate
qualified producer through OpenRouter and the shared ledger; this local contract
does not authorize that extension or forgive uncertain liabilities.

## Evidence and alternatives

The [predeclared grading](../evaluations/p10-03-observer-grading.md) distinguishes
diagnostic evidence navigation from task-level benefit. Grouping exact references
can justify an opt-in diagnostic without proving that interventions improve task
completion. Default enablement remains unjustified without held-out outcomes.

An always-on loop adds scheduling and recovery ownership without demonstrated
need. Reusing the model child contract would require fake model policy and
monetary allocation. Enabling a statistical filter would contradict its existing
qualification. These alternatives are not selected.
