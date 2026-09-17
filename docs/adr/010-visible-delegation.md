# ADR-010 — Visible bounded delegation and integration

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P7-04/05/06. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#16-multi-agent-work-and-integration) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Delegation is required capability, selected when useful. Children share root accounting and inherited authority ceilings, have visible progress, and pause with their owner. Writes are isolated or serialized.

## Implementation proposal

Persist bounded child specs with scope, role, objective, dependency graph, dirty-base snapshot, acceptance criteria and allocation. Materialize intended staged/unstaged/untracked state, not just HEAD. Integrate validated child packets through prepared edits against current parent versions, then verify the integrated result.

Detailed contracts and failure ordering are in the [supporting design](../architecture/routing-extensions-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Use Git worktrees for qualified repositories and explicit snapshot/serialized ownership for non-Git folders. Worktree cleanup needs ownership and reference checks. Read-only helpers need the same context/memory scope as write children.

## Qualification evidence

E12/E15/U02/U03/U06 observe actual concurrent effects, dirty-state preservation, budget races, malformed packets, stale integration, child output floods and root terminal close. Passing child checks cannot qualify a different parent fingerprint.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Isolation adds disk and integration cost; duplicated work may erase routing savings. Record full child costs/results even after cancellation. Reconsider scheduling based on measured benefit while retaining visible attributed activity.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
