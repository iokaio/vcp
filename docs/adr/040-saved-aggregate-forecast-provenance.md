# ADR-040 — Saved aggregate forecast provenance

Status: accepted for P6-05 / M3, 2026-09-20.

## Context

Optimizer forecasts combine authorized evidence from several tasks. Canonical
artifacts have a task scope, and the store correctly rejects their direct
references to another task's evidence. Assigning the aggregate to the first task
also cannot authorize a reader of that task to read the complete aggregate.
Historical forecasts must retain exact bytes and be removed with their sources.

## Decision

Publish the forecast artifact with an immutable typed workspace source manifest,
`vcp_optimization_forecast_sources_v1`, and an optional report pin. The manifest
declares the exact contributing tasks, records and events, source commitment,
report identity and artifact digest. Store validation checks the declared scopes,
references and artifact binding. It grants no execution or reading permission.

The artifact references its own task and the workspace manifest. The manifest
references the cross-task sources. Ordinary task reference validation remains
unchanged. Report, manifest, artifact descriptor and attachment event publish in
one transaction after spool finalization. A pre-publication interruption can
leave unreachable spool bytes, but no partially published canonical report.

Generic artifact reads reject `vcp-optimization-forecast-v1`. The dedicated
loader checks every source task under current access, authority and deletion
state, validates retained sources and hashes, and returns the saved bytes. It
never silently reconstructs a historical prediction. Existing reports without
the optional pin remain readable and report unavailable forecast comparisons.

Retention follows manifest record and event dependencies to the aggregate.
Purging replaces the manifest with a sealed content-free typed tombstone and
purges the derived artifact. Ordinary writes cannot mutate or resurrect the
manifest. This uses the existing deletion epoch and sealed rewrite mechanisms.

## Consequences

Multi-task reporting remains possible without relaxing generic cross-task
artifact rules. Aggregate reading requires the specialized loader, and loss of
any required source makes the snapshot unavailable. The new typed manifest and
tombstone add a narrow storage/retention contract. Forecasts remain unqualified
inspection data with no routing, budget or permission authority.

Verification and limits are recorded in
[saved forecast diagnostics](../evaluations/p6-saved-forecast-diagnostics.md).
