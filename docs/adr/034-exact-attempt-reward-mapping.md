# ADR-034 — Exact attempt-visit reward mapping

Status: selected for the P6-02/M1 reward-mapping increment. This defines an
unqualified cost-reward artifact over retained attempts. It does not infer task
value, forecast total cost, persist a fit or serve a routing estimate.

## Evidence and problem

ADR-031 attributes exact terminal charges to individual attempts, while the
bounded Markov kernels accept one nonnegative reward per modeled visit. A safe
bridge must define that visit and cohort precisely, preserve currencies and
supporting roles, and prevent an incomplete provider outcome from becoming a
zero-cost sample. Task/root ledger rollups cannot be combined with their attempt
components.

## Decision

Add `routing_state::rewards::map`, a read-only rebuild from one authorized
`canonical-action-observation/2` evidence view. One attempt is one model-cycle
visit under `attempt-visit-exact-cohort/1`. Its reward is the terminal nonnegative
charge in micros under `terminal-attempt-charge-micros/1`.

Cells keep exact role, model, endpoint, admitted policy, task class availability,
root/child identity, retry/decomposition depth, prior attempt/failure counters and
currency separate. This prevents a retry, helper, child or verification request
from being merged into another component or counted again through a task/root
rollup.

Each cell records total, exact and unknown attempts; exact charge sum and observed
range; available cumulative charged and reserved-liability totals; and the
upward-rounded exact sample mean. Under `complete-cohort-or-abstain/1`, the mean
exists only when every attempt in that exact cell has a complete terminal charge.
Any missing prefix, pruned source, legacy observation, open/uncertain outcome or
reserved liability withholds the cell mean. Available charged and liability sums
remain separate observations; they are not presented as a bounded final cost.
An exact no-send release remains a legitimate zero sample.

The version-one artifact binds the source evidence ID/digest, cutoff/window,
authority/deletion/scope, alphabet, gap/censoring/pruned coverage, feature and
cohort definitions, current policy/catalog identities, algorithm and uncertainty
method. It records no attempt IDs or prose, writes nothing, has no qualification
ID and sets `serving_qualified = false`.

## Consequences and remaining work

M1 can now supply exact per-visit monetary samples to later absorbing-chain work
without hiding unknown liability or mixing cost components. It still does not
assign monetary value to success, combine roles into total task cost, establish a
confidence interval or qualify a forecast.

Consumed-value persistence and exact replay remain before the M1 foundation is
complete. M3 owns read-only forecasts; M4 owns frozen qualification and any
activation. Verification is recorded in the
[increment evidence](../evaluations/p6-reward-mapping.md).
