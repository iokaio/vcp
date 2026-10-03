# ADR-081 — Reusable provider adapter compatibility

Status: accepted October 2, 2026 for BETA-03C.

## Decision

The owner approved model-set setup with one small connection prompt. The
two-request, short-lived, per-model qualification workflow cannot establish
account defaults for a set without repeated paid probes. Separate three claims:

1. Reusable adapter evidence describes the implemented OpenRouter Responses
   text/tool codec and strict provider request fields. Its identifier hashes the
   checked-in request, stream and catalog codecs. The reviewed contract is dated
   October 2 through December 31, 2026; a software update must renew an expired
   contract deliberately. It is not empirical per-model qualification.
2. Fresh complete endpoint metadata validates each exact model, endpoint,
   parameter support, context/output limits, availability and tariffs. Its
   lifetime is at most twelve hours and never exceeds the adapter evidence.
   Missing, unsupported or ambiguous metadata fails closed. Price changes are
   parsed afresh and affect reservation without requiring a new paid probe.
3. A user's connection test verifies one credential's text response from the
   displayed model, response decoding and a provider-reported charge settled in
   canonical accounting. It does not verify tools, every member of the set,
   coding quality, tokenizer bounds or downstream provider attribution.

The empirical `responses_text_tools`, `provider_preferences_qualified` and
`byte_ceiling_qualified` flags remain false for adapter-contract snapshots.
Admission recognizes the exact compiled contract and its restrictions through
one validator. Invented contract IDs, altered restrictions, expired metadata and
unsupported capabilities remain rejected. Existing empirical qualification
records and the explicit two-probe command remain supported.

This supersedes ADR-041's requirement for a fresh paid per-model probe for this
adapter-contract lane. ADR-041's monetary and attribution distinctions remain:
full-input/cache/output/request reservation, strict parameter filtering, exact
provider restrictions, disabled gateway fallback, denied data collection,
bounded execution, unknown-liability retention and honest absent attribution.

The single connection request allows 128 output tokens and no inference retry.
Setup persists a pending attempt before dispatch. Interrupted or unresolved
accounting blocks another attempt; a successful matching retained result can
finish setup without a second paid request. A fully settled failed attempt may
be followed only by a newly authorized test. Evidence is retained independently
of the completion marker.

## Evidence and limitations

The provider documents a stateless Responses interface for text and tools and
explicit provider routing controls. See the primary
[Responses overview](https://openrouter.ai/docs/api_reference/responses/overview)
and [provider selection contract](https://openrouter.ai/docs/guides/routing/provider-selection).
Synthetic adapter, metadata, response and accounting tests support the compiled
contract. These are not a live compatibility or comparative model campaign.
The gateway may reject a model that advertises the required parameters; such a
failure remains bounded and cannot authorize an outside-set model.
