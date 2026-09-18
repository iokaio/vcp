# ADR-020 — Bounded semantic decisions

Status: proposed engineering design; qualification pending. Existing product
requirements remain binding. No new provider, default or runtime is accepted.
Decision gate: P6-02/03/04/05; P7/P8 verify downstream integration under their
existing dependencies. No additional product task or release prerequisite.

## Context and authority

The [JEV exploration](../architecture/exploring-jev.md) proposes separating bounded
semantic judgments from generative coding work. Routing, escalation, review triage
and optimization can consume typed advice without asking a model to control the
workflow. The existing [routing contract](../architecture/vcp-what.md#75-routing-algorithm-and-explanation)
already requires explainable rules, measured quality, bounded spending and complete
evidence. This proposal develops that seam before P6 implementation.

## Proposed decision

Introduce a vendor-neutral `DecisionEvaluator` boundary, logically `vcp-decision`,
for closed Boolean/Choice/Score questions, explicit abstention and provenance.
Rust code owns validation, composition, policy, budgets, authority, persistence and
side effects. A score or probability never grants permission or proves completion.

Start with deterministic rules and a disabled optional OpenRouter implementation.
Any remote evaluation, retry, correction or shadow trial uses the existing gateway,
context policy, root ledger and pause/recovery barriers. Select helper models
without recursive evaluation. Accept optional advisory inputs only through explicit
versioned rules; preserve deterministic fallback and an inspectable explanation.
Enable a measured policy only after held-out P6-04 qualification and a recorded
configuration change. No vendor performance claim selects defaults.

The [supporting design](../architecture/decision-evaluation-design.md) defines
record semantics, strict validation, admission/recovery, consumer limits and
evaluation. The [plan](../plan/12-routing-and-optimization.md) assigns construction
to existing tasks. `vcp-policy` does not become a model client and the decision
package does not own a second engine, canonical store or cost ledger.

## Alternatives and deferred proposals

- Deterministic rules alone remain the baseline and a valid shipping configuration.
- Unconstrained model orchestration loses closed outputs, reproducibility and clear
  effect ownership; it is not selected.
- A direct Jev endpoint would change the confirmed OpenRouter boundary. It remains
  unselected pending an explicit owner scope decision, data/terms and provenance
  review, provider/accounting support and measured value. No credentials, SDK or
  second gateway are introduced by this ADR.
- A local semantic model needs separate licensing, pinned assets, resource and
  Windows qualification. It is not implied by existing embedding support.
- A Python adapter port is only a source candidate. No code or model import is
  authorized by a research link; exact-revision provenance precedes any borrowing.

Local retrieval/indexing and memory governance remain unchanged. Optional memory
extraction may later reuse the contract without making earlier P5 tasks depend on
P6. Background observers remain deferred P10 work. Required tests, reviews, full
history, authority and user-selected policy changes cannot be removed by advice.

## Qualification evidence

Use E11/E12/E19/U07 with offline schema, injection, zero-call, budget, stale-result,
pause/recovery and fallback fixtures. Distinguish probability calibration from
scores or discrete predictions. Compare matched deterministic and OpenRouter
policies on held-out tasks, including failed/repair/helper cost, quality, latency,
missed reviews/escalations and interventions. P8 verifies enabled/disabled behavior
with actual delegation and Windows recovery. Tests and evaluations remain planned.

## Consequences and reconsideration

A shared contract can avoid duplicated classifiers, but may add cost and latency
without improving decisions. Retain abstention and a no-helper path. Question/model
changes invalidate relevant qualification; regressions or insufficient evidence
keep or restore the deterministic policy. Thresholds, cohorts, exact defaults,
probability tolerances and any upstream import remain open engineering decisions.
