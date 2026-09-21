# ADR-031 — Exact attempt charge attribution

Status: selected for the P6-02/M1 charge-reward increment. This extends the
action projection selected by ADR-030. It does not fit a model, aggregate charges
into a state reward or authorize a routing consumer.

## Evidence and problem

Canonical attempt and reservation snapshots retain cumulative charge and current
liability. Immutable settlement records retain whether each provider observation
was applied, its debit/credit adjustment and its resulting cumulative total.
Before this decision, `UsageReconciled` events did not carry the settlement
identity, so a historical reader could not associate the event with its exact
immutable settlement without guessing from current rows. A current cumulative
attempt total also cannot prove that every retained adjustment is present.

Task/root ledger totals are rollups. Adding them to attempt charges would count
main, retry, helper, child and verification work twice. A reservation or unknown
provider outcome is a liability, not a zero-cost visit.

## Decision

New `UsageReconciled` version-one event payloads add an optional typed
`settlement` reference containing only schema version and immutable ID. Existing
events remain readable. The referenced record's attempt/scope, provider request,
raw receipt artifact, currency, normalization, applied flag, direction,
adjustment and cumulative total must agree with the event's attempt snapshot.
Repeated observation IDs remain idempotent and append no second event.

Advance the read-only projection to `canonical-action-observation/2`. Every
attempt trace has one `ChargeAttribution` with:

- the quote currency and owning attempt;
- retained debit/credit/none settlement observations in canonical append order;
- the cumulative charged amount and current reservation liability when their
  retained sources are complete;
- an explicit completeness and unknown-remainder state; and
- an exact terminal `final_charge_micros` only for a complete settled attempt or
  a complete no-send release.

The terminal charge is the nonnegative cost-reward component later numerical
work may attribute to the preceding attempt visit. The projection does not mix
currencies, convert money, divide a charge among states or add task/root ledger
rollups. Role and root/child identity stay on the attempt cohort, so downstream
work can select components without double counting them.

Late applied observations remain separate settlement facts. Debits and credits
must reconcile from the prior retained cumulative total; the last complete final
observation supplies the terminal total. An idempotent, unapplied observation has
zero adjustment and cannot add cost. Missing prefixes, gaps, old events without
the settlement field, pruned settlement/reservation rows and explicit remaining
uncertainty make the point estimate unavailable. `None` means unknown and is
never interpreted as zero. Reserved liability remains visible separately when
its source is retained.

## Retention, privacy and compatibility

The event adds no provider response or correction prose. The projection exposes
integer monetary facts and IDs, not provider responses, raw usage artifacts,
correction reasons or uncertainty prose. It checks current
authority and logical/physical retention for attempt, reservation and settlement
sources on every rebuild. A purged settlement cannot survive through the event's
copy or cumulative attempt field.

The additive event field keeps old canonical stores compatible. Old usage events
still yield their action phase, but charge attribution is incomplete and has no
terminal point estimate. Projection schema/alphabet version two makes the new
required output fields explicit instead of silently changing version one.

## Consequences and remaining work

M1 now has exact per-attempt charges, late-adjustment identity and explicit
liability abstention. ADR-032/033 subsequently add source-bound fits and held-out
order comparison; ADR-034 maps complete charges to exact attempt-visit rewards.
Consumed-value replay remains. M2/M3 consumers remain disabled.

Verification commands and results are recorded in the
[increment evidence](../evaluations/p6-charge-rewards.md).
