# ADR-033 — Task-separated held-out Markov order comparison

Status: selected for the P6-02/M1 held-out comparison increment. This defines a
local model-order diagnostic. It does not statistically qualify, persist or serve
a routing estimate.

## Evidence and problem

ADR-032 binds a first-order candidate to retained evidence, but selecting that
order without an out-of-sample comparison would hide history leakage and excess
second-order complexity. M1 also requires a multi-step check against observed
frequencies. Sparse held-out contexts and outcomes must cause abstention rather
than receive invented probability mass.

## Decision

Add a bounded pure `compare_orders` routine and a read-only
`routing_state::fits::compare` adapter. The adapter rebuilds
`canonical-task-state/1` evidence and assigns whole task traces with
`task-identity-digest-modulo/1`. The modulus and held-out bucket are explicit.
One task can occur in only one cohort, and adding another task cannot reassign an
existing task. Gaps split a task into contiguous segments and never create an
edge.

Both orders score exactly the same held-out next-state predictions. The
first-order candidate conditions on the current state; the second-order candidate
conditions on the preceding and current states. Priors apply only to outcomes
seen in the corresponding training row. Missing contexts, unseen outcomes and
rows below the declared raw sample gate abstain.

The comparison records held-out log likelihoods and a BIC-style score:

`held-out log likelihood - 0.5 * supported free parameters * ln(training predictions)`

Each supported categorical row contributes `observed outcomes - 1` parameters.
Ties select first order. The artifact also records the maximum absolute error
between held-out two-transition start/end frequencies and the first-order `P²`
prediction. A missing intermediate training row abstains rather than losing
probability mass.

The version-one artifact binds the source evidence ID/digest, cutoff/window,
authority/deletion/scope, alphabet, gap/censoring/pruned coverage, available
policy/catalog identities, feature/cohort dimensions, algorithm and partition
identities, cohort/segment counts, prior and sample gate. It contains either finite
comparison results or a closed abstention reason. It is never persisted and always
has no qualification ID with `serving_qualified = false`.

## Consequences and remaining work

The digest split is deterministic and useful for local regression evidence. It is
not the project-separated and time-separated frozen split required for final
qualification. Small or unrepresentative cohorts can abstain or produce a noisy
diagnostic. No comparison result changes runtime routing.

M1 still needs reward mapping and uncertainty qualification plus consumed-value
retention/replay. M4 owns final purpose-specific qualification and any activation.
Verification commands and results are recorded in the
[increment evidence](../evaluations/p6-heldout-order-comparison.md).
