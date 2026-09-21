# ADR-041 — Provider attribution evidence and conservative routed admission

Status: selected for P6 completion; verification and live dispositions are recorded
in the P6 completion evidence. Supersedes the fixed-provider-only fallback and
stream-identity-only qualification assumptions introduced during P6 bootstrap.

## Evidence and decision

The successful synthetic Responses probes omitted served-provider identity.
OpenRouter's read-only generation records independently bind a response ID to
one successful provider attempt, an internal endpoint UUID, model revision and
observed charge. The full dated public endpoint catalog carries provider display
names and exact external routing tags, but does not expose those UUIDs.

Qualification may join a generation receipt to a completed, cost-accounted probe
only when response IDs and rounded charges agree; requested model identity is
confirmed by the Responses payload; the receipt names exactly one successful
non-BYOK provider attempt; and the complete captured model catalog has exactly
one endpoint with that provider name and the requested exact tag. Any regional,
variant or duplicate-name ambiguity rejects. The selected endpoint's `model_id`
must equal the requested alias and its bounded `name` must exactly equal the
observed provider name, ` | `, and the generation's observed model revision.
A generation and its provider-attempt record agreeing on an unrelated revision
is insufficient. Missing catalog revision metadata fails closed. Both probe responses must observe
the same internal UUID and immutable model revision. The raw absent stream
identity stays absent. The artifact labels this as a derived catalog association,
retains the internal UUID separately, and binds every input hash. It does not
claim that a public catalog exposed the UUID.

The offline qualification command reconstructs the production snapshot through
the existing raw-catalog parser. A newly observed catalog with unchanged required
parameter support, context/output limits and normalized tariffs may refresh
price freshness, but this probe's compatibility evidence expires at most 24 hours
after its original observation. Any relevant capability/tariff change, differing observed revision,
ambiguous provider mapping or receipt conflict requires fresh qualification.
No script derives quality, group membership, reasoning effort or tokenizer
qualification from these protocol probes.

Routing now permits an explicitly unqualified byte estimate when the existing
canonical admission path reserves the endpoint's full input capacity for each
possible input/cache partition, plus output and request maxima. The flag remains
false. Selection also checks that conservative immediate bound independently
from the estimated task cost and protected verification reserve. Final admission
still uses the captured snapshot and current root balance atomically.

Context bytes remain an estimate used to reject obviously oversized inputs;
they are not a monetary bound or tokenizer guarantee. A provider context rejection
is an observed task failure subject to existing retry limits. This fallback is
more expensive in reserved capacity but preserves the same hard spending bound
for fixed and routed requests without requiring empirical tokenizer claims.

## Alternatives and consequences

Treating a requested pin as proof of who served a request was rejected. Accepting
a display name without checking all catalog variants was rejected. Inferring a
global token bound from a small multilingual probe was rejected. Blocking all
routing despite an available conservative hard bound was unnecessary.

The join relies on the gateway's own authenticated metadata, just as the original
snapshot relies on gateway catalog and usage observations. It does not provide
independent attestation of a downstream provider's internal infrastructure.
Exposed aliases and observed immutable revisions remain distinct, dated evidence.
The mandatory user/provider restrictions, strict parameter filtering, disabled
fallbacks, data policy, unknown-liability retention and final reservation do not
change. Negative qualification outcomes continue to leave a candidate disabled.

Sources: [provider routing](https://openrouter.ai/docs/guides/routing/provider-selection),
[generation metadata](https://openrouter.ai/docs/api/api-reference/generations/get-request-&-usage-metadata-for-a-generation),
and the retained [P6 retry observations](../evaluations/p6-live-provider-retry.md).
