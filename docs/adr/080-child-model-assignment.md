# ADR-080 — Explicit child model assignment

Date: October 2, 2026. Status: accepted by explicit owner direction; implementation in progress.
Owning item: BETA-12 in the [release plan](../release-planning/00-release-plan.md).
Extends [ADR-077](077-guided-cli-setup.md).

## Context

Guided setup offers model sets. The owner chose explicit per-role assignment
over automatic routing: a set names the exact model each kind of work uses, and
no quality evidence or measured routing defaults are required
([ADR-007](007-profiles-and-routing.md) still governs automatic routing, which
has no live-qualified groups).

Investigation of the engine fixed the shape of that choice:

- A **main** request and a **delegated child** request already select a
  `Snapshot` per request, and every attempt is quoted, admitted and checked
  against its own snapshot. Only the fixed-selection fallback assumed one model.
- **Compaction** makes no model request. VCP compaction is a local projection,
  and a model-initiated compaction request is rejected. There is nothing to
  assign a compaction model to, so the earlier idea of a compaction member in a
  set is dropped.
- Children are started from the interactive terminal (`/agents`) and run in the
  same canonical owner and provider as their parent. Their retained thread is
  configured with a single model slug, which the admitted request body must
  match.

## Decision

- A profile may assign **one additional model to delegated children**:
  `roles.child = { provider: <qualified Snapshot>, catalog: <endpoints.json> }`.
  Main work keeps the existing `provider` and `catalog`. No other role is
  assignable, and `roles` is mutually exclusive with automatic `routing`.
- The child model is qualified exactly like the main model: a live catalog
  capture and the accounted two-call probe. Profile preparation re-derives the
  snapshot from its catalog and requires it to be current. The host's output
  ceiling must fit both models.
- The engine's `configure_child_provider` records the assignment
  (`openrouter-role-provider-configuration/1`) after the main provider. It is
  refused with automatic routing, when the model equals the main model, when
  the snapshot is stale or differs from its catalog, or while attempts are
  active. Reconfiguring the main provider clears it.
- **Selection is by recorded assignment, not by role.** Delegating a child
  records the model it will use (`model_policy`): the child model when one is
  configured, otherwise main. A nested child inherits its parent's model. Each
  request then selects the configured snapshot matching that recorded model.
  A child delegated before an assignment existed keeps running on main, and a
  child whose recorded model has no configured snapshot is rejected before any
  send or reservation, as today.
- The CLI starts a child's retained thread on the child's recorded model, so
  the thread's model and the admitted body agree.
- **Resume rule.** A resumed root task uses the currently selected profile's
  main model; each attempt records its own model and price, and the served
  model must equal the admitted one. A child stays pinned to the model it was
  delegated with; resuming it under a profile that lacks that model fails
  closed with no reservation.

## Consequences

- Model sets become main + child. Each distinct member costs one verification
  pair; `vcp setup estimate` sums them.
- Attempts for children are priced at the child model's rates and remain
  attributable per attempt in the ledger.
- Automatic routing and child assignment cannot be mixed in one profile; a user
  who wants routing removes `roles`.
- Changing the assigned child model requires a new profile; existing children
  are unaffected until re-delegated.
