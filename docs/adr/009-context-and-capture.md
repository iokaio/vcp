# ADR-009 — Versioned context and full activity capture

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P1-03, P2-08, P3-03. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#6-context-instructions-and-compaction) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Retain full observed work locally while constructing bounded, reproducible prompt projections. Compaction cannot erase history or elevate evidence to instructions. Credentials and recovery secrets are excluded from capture boundaries.

## Implementation proposal

Each context part records source identity/version, scope, trust class, artifact, selection reason and omitted ranges. Seal a manifest against task, steering, policy, model, tools, skills and memory revisions. Refresh before dispatch; invalidate stale prepared work. Preserve compatible tool/result pairs on compaction and handoff.

Detailed contracts and failure ordering are in the [supporting design](../architecture/context-provider-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Reuse scoped discovery and upstream context machinery behind VCP manifests. Deterministic summaries may be local; model-assisted compaction is a recorded, budgeted attempt. Define capture exclusions at typed secret inputs and report omissions rather than claiming universal heuristic redaction.

## Qualification evidence

E02/E04/E17/R07/U05 test late user corrections, hostile retrieved instructions, smaller model envelopes, output beyond UI limits, disk-full capture and inspector replay. Original complete artifacts must remain available subject to current retention/access.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Full capture increases local disk and privacy exposure; visibility and pruning are required. Unrecordable effects cannot continue as if capture succeeded. Unsaved editor drafts remain a later explicit capture policy, not automatically settled memory.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
