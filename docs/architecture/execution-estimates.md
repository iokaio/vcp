# Execution estimate accounting (EE-01)

Reservation estimates and observed charges have different meanings. Actual charges and settled totals remain finite integer micro-units. An estimate may contain unpriced valuation terms; those terms never become zero-priced services or settled charges.

## Wire representation

Existing fully priced amounts retain their canonical decimal-string representation, for example `"1200"`. An unpriced estimate is explicit:

```json
{"kind":"unknown","version":1,"known_component":"1200","unknown_components":"2"}
```

`known_component` is the priced portion of a conservative estimate, not a lower bound on the eventual bill. `unknown_components` counts missing category valuations; aggregation adds those terms across reservations, rather than counting reservations. A missing category tariff remains unpriced even when that request's bound for the category is zero, preserving the finite contract's requirement for complete tariff metadata. This count does not describe consumed units or actual charges. It must be positive. Decoders reject extra fields, unsupported versions, noncanonical counters and overflow.

This representation applies to quote amounts, reservation amounts and liabilities, ledger active/unresolved estimates, and public reserved/unresolved usage totals. Actual charged amounts and settled totals remain ordinary finite money. High-level report aggregates whose inputs include unpriced terms expose a null total; canonical reservation evidence retains the exact components.

## Admission and reconciliation

Finite admission requires a fully priced estimate. Explicitly unbounded admission may retain an unknown estimate, while the existing scope, authority, current revision, request digest, attempt identity, send intent and duplicate-send checks remain mandatory. Unknown estimates cannot draw finite protected funds. An unpriced numeric component of zero is still unresolved liability.

A partial observed charge reduces the numeric estimate component with the existing saturating subtraction, but does not price missing terms. Final observed settlement, proven pre-send release, or the existing governed explicit-resolution operation terminates liability. Reopening reconstructs these exact fields and never manufactures another send permit. The separately retained proof for a completed response with missing cost affects reporting only; transport or effect uncertainty remains fenced.

## Captured provider pricing

Legacy tariff interpretations and their request bytes remain readable. The effective owner policy is derived from the original captured metadata, with independent capability and identity validation. Explicitly unbounded execution uses the versioned `endpoint-observed-estimates/3` interpretation. Missing prices remain absent, malformed supplied tariffs still fail, and the request omits financial `max_price` preferences. Endpoint pinning, fallback restrictions, privacy requirements, parameter qualification and provider context/output ceilings remain unchanged.

The shared provider and routing configuration boundaries capture this effective interpretation and its distinct identity. Routing publication uses the existing catalog revision flow. Original accepted task facts and raw catalog evidence are preserved. Finite connection probes reconstruct the finite interpretation and require a known reservation before any send.

Routing diagnostics retain partial estimates, and finite routing excludes unknown totals. Where cost ordering is requested, known estimates precede unpriced totals; identity and the remaining declared preferences deterministically resolve ties. Owner choice-set membership remains explicit. For unbounded execution, price ceilings do not remove members before launch; captured reference estimates retain unknown components and the effective monetary ceiling is explicitly unbounded.

This changes the existing execution path and its data contracts. It does not introduce another driver, send mechanism or execution authority.
