# ADR-085 — Refine the existing execution engine for quality and diagnostic understanding

Status: accepted October 4, 2026 by explicit owner approval of the [execution architecture plan](../architecture/execution-architecture-review.md). Implementation is tracked in [EE-00–EE-07](../plan/25-execution-engine-refinements.md).

## Decision

Modify the existing execution path on `feature/execution-engine-refinements`. The shared retained driver schedules work and the canonical worker owns durable transitions. Do not create a collection-mode engine, legacy/new selector or independent orchestrator. Result quality is primary; sufficient diagnostic capture and analysis is a very important secondary priority. Fast state access, useful context and adaptive input/output limits remain active implementation work.

Suspend task/scenario deadlines, monetary affordability and unknown-charge refusal gates, synchronous financial settlement dependencies and automatic evidence cleanup while collecting execution data. Represent suspended limits explicitly as Unbounded rather than numeric sentinels. Preserve legacy accepted evidence and record transitions when older tasks resume. This supersedes conflicting task-limit and affordability requirements in P1-05/P2-07 and ADR-083/084 for this branch; it does not delete financial observations.

Preserve reservation/attempt identity, no-send versus uncertain-send records, duplicate-send fences, currency/unit/overflow validation, current tool authorization, provider capacity, explicit cancellation and uncertain-effect containment. Financial uncertainty is not uncertain tool authority. Retain evidence for later analysis; no new retention scheme is required.

Schedule verification and repair through the existing shared driver. Refresh instructions before any affected tool can execute. Use typed completion outcomes and fresh full-set verification before completion. Focused checks remain a subset of explicitly configured requirements and cannot alone complete a task.

Use Paused plus versioned diagnostic reason evidence for deterministic repeated failure without progress. This explicitly extends ADR-067's observation-only policy for the new controller action, while preserving ADR-029/030 observation/signature semantics and ADR-016/045 deliberate-resume ownership. Explicit cancellation remains terminal Cancelled.

Preserve semantic corruption detection on reopen. Incremental validation must agree with the full validator on every generated accepted or rejected transaction. A local snapshot hash alone does not authorize skipping semantic history validation; equivalent checkpoint integrity must be established separately within EE-02d. If it cannot be established, preserve validation and report that target outstanding while delivering other state improvements.

## Compatibility and testing

Update shared schemas, settings, generated TypeScript, SDK/editor clients and validators together. Keep history and original digests intact; use explicit versioned formats and audited continuation records. Classify affected tests by retained invariant versus superseded limit behavior, with replacement coverage rather than blanket skipping.

Use deterministic tests, scripted faults, small live experiments, full A/B and larger engagements through the same path. Collect and analyze both successes and failures. Every completed phase has a local commit and truthful evidence. No push, PR, release or publication is authorized.

## Deferred decisions

Selected restrictions may be added later after sufficient engagement data and owner decision. Their restoration machinery and thresholds are not prerequisites for the improved engine. EE-08 is not implicitly activated by implementation approval because its evidence gate and separate decision remain part of the approved plan.
