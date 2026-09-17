# ADR-012 — Client sequencing and native distribution

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P8; later P9/P4/P10. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#19-configuration-packaging-and-operations) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Deliver the native Windows CLI and complete first-release feature set before external API, SDK, editor or other hosts. Prepare downloadable Apache-2.0 artifacts with actual applicable attribution; publishing is a separate authorized action.

## Implementation proposal

Build one coherent native artifact set with CLI/worker/runtime compatibility metadata, skill catalog and model provisioning records. Test clean installation, data-root placement, diagnostics, recovery and migrations using packaged bytes rather than a development checkout.

Detailed contracts and failure ordering are in the [supporting design](../architecture/qualification-release-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Select archive/installer formats, minimum Windows/CPU/RAM and signing workflow from P0/P8 evidence. Reuse retained Codex packaging where compatible. No certificate, hardware support floor or bundled model redistribution right is assumed.

## Qualification evidence

E18/R04/U04/U09 and P8-04 require a clean user profile, missing dependencies/assets, locked executable, interrupted upgrade, incompatible state and fresh-machine recovery. Artifact hashes identify the tested package.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Binary rollback may not undo state migrations. Preserve data, independent keys and user projects on uninstall. New client/platform support needs its own host and compatibility evidence, without changing engine ownership.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
