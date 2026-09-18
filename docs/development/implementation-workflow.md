# Implementing a VCP task

Status: proposed engineering workflow supporting all plan segments. Product requirements remain in the [architecture](../architecture/vcp-what.md); task ownership and prerequisites remain in the [ledger](../plan/20-traceability.md). This document creates no additional product task.

## Prepare a task packet

Record these inputs before changing production behavior. Keep private fixture locations and credentials out of tracked packets.

| Input | Required content |
|---|---|
| Identity | One owning task ID, intended observable behavior, source revision and dirty-diff identity |
| Readiness | Exact prerequisite IDs and current evidence; partial prerequisites are named blockers to claiming completion |
| Authority | Architecture anchors, ADR status and open choices relevant to this increment |
| Placement | Logical service and actual package/module paths from the P0 source map |
| Contract | Inputs, outputs, scope, invariants, error categories and compatibility version |
| Effects | Canonical transaction, artifact, external dispatch and acknowledgement order |
| Validation | Test IDs, fixtures, independent oracle, platform/asset requirements and expected observations |
| Operations | Reopen/retry, migration, cancellation and inspector consequences |
| Exclusions | Deferred surfaces and explicitly unfinished portions of the owning task |

A pure contract prototype can precede a dependent production integration without marking the dependent task ready or complete. Record what the prototype substitutes. For example, a scripted provider exercises reservation ordering but cannot qualify served-model compatibility.

## Split work by behavior

The steps below are construction increments within a milestone, not one PR per
step. Prefer finishing the connected behavior and its failure cases locally
before opening or updating the milestone PR.

1. Add domain/interface changes with errors and revision semantics. Define what a stale input means and which component may issue an authoritative receipt.
2. Wire one real caller through the interface. Keep retained upstream behavior and tests when applicable; avoid adding a parallel controller for convenience.
3. Implement the happy path with actual durable writes and bounded resource handling.
4. Inject a failure at the boundary where a false success, duplicate effect or unauthorized access could occur.
5. Add projection/CLI consequences so a user can distinguish pending, failed, unknown and complete.
6. Record tests against the resulting source state. Only then broaden to other backends/platform variants that the task promises.

Do not treat a source-file checklist as a task's exit criteria. A trait, mock, migration file or empty directory alone demonstrates no behavior.

## Delivery milestones

Choose a primary owning task and name any related tasks whose exact prerequisites
are satisfied. Define an observable end-to-end result, its failure boundaries,
and the checks needed before publication. Use the [planned foundation groupings](../plan/README.md#pr-milestones-and-local-validation)
as the current starting point. Preserve each task's ownership and completion
criteria even when one PR covers several related increments.

Work on one branch from current main. Accumulate interface changes, real callers,
state transitions, cancellation/recovery behavior, fixtures, upstream patches,
notices and documentation together. Use local commits as checkpoints, not as a
reason to create another PR. Do not stop at a trait or positive-path probe when
the next connected implementation and failure checks can be completed safely.

Run focused tests during development, then the applicable combined local suite
against the final source. Prefer existing build caches with recorded inputs;
reconstruct selected upstream patches independently when their source changes.
Run native Windows checks locally for affected process/lifecycle behavior and
report missing prerequisites as not run. Do not use repeated GitHub pushes as
the debugging loop. Hosted heavy qualification remains deliberate and manual;
the fast hosted check confirms repository portability after publication.

Before publishing, review the full milestone diff, test evidence, provenance,
plan status and user-visible limitations. Publish a PR describing the final
behavior rather than the sequence of prototypes used to reach it. Fix relevant
review or CI failures, then follow the already authorized merge process and
update main before starting the next milestone. Do not relax product acceptance
or claim native qualification from a skipped hosted job.

## Shared service design worksheet

For each interface document ownership, lifetime, side effects and serialization separately. A proposed Rust trait is useful only after these questions have answers:

- Which stable IDs and expected revisions enter the call? Which are engine-generated?
- Is the returned object an untrusted proposal, a durable receipt or a derived projection?
- Which lock/transaction protects admission, and what work must happen after that lock is released?
- Can the caller safely repeat the call after a timeout? Which payload digest detects conflicting reuse?
- What persists if cancellation occurs before dispatch, during the external effect or after the effect before acknowledgement?
- Which current access/deletion/policy revision is checked before content or effects leave the service?
- How are limits surfaced, and which event/artifact lets an inspector explain the result?

Use distinct types for proposal, prepared operation, admitted attempt and finalized encrypted object. Public constructors must not let model-derived data manufacture the later states. Serialization alone does not establish that a receipt is genuine; validate its store identity and referenced revision.

## Revision and compatibility discipline

Separate source schema, canonical data format, index format, context/skill/tool schema, provider compatibility and CLI event versions. An engine binary version does not prove all of them compatible.

Add migrations forward from the applied schema, with interruption and recovery behavior. Index-only incompatibility should rebuild from retained canonical/source/vector inputs; authoritative-record incompatibility needs validated migration or an actionable refusal. Restores never reactivate source-host authority or reset liabilities.

Every verification record identifies the files/configuration/environment it actually checked. A relevant later edit makes it stale; unrelated changes may retain applicability only if the dependency scope is explicit. Final summaries preserve earlier failed and not-run checks.

## Review a cross-service change

Use the following end-to-end walk when a change touches scheduling, effects or data eligibility:

```text
command + expected revision
  -> authoritative admission and idempotency lookup
  -> payload capture / prepared resource identities
  -> current authority + budget / retention checks
  -> durable intent and reservation
  -> external dispatch outside transaction
  -> observed outcome and immutable artifacts
  -> atomic receipt / settlement / indexing intent
  -> derived projections and user-visible result
```

Some operations omit model budget or external dispatch; record that explicitly. Never move an external call into a retrying store transaction. A lost response after commit returns the old result on retry; a lost response after an uncertain external effect requires reconciliation.

Ask a second reviewer to challenge one race at each relevant boundary: steering while queued, revoke during retrieval, edit before apply, pause before send, prune before backup activation, or crash after effect. The [subsystem designs](../architecture/README.md) own the detailed algorithms.

## Evidence and public documentation

Keep full run manifests and raw traces in ignored local artifacts or an explicitly private output root. A public summary contains synthetic/public fixture identities, sanitized commands, versions, outcome counts and limitations. Hashes of private inputs may still identify private material; include them only in appropriately scoped local evidence.

An evidence reference binds the exact source and dirty state that ran, not just a branch name. Package tests additionally bind the final artifact digest. Record fail/not-run separately and preserve retries rather than replacing the failed attempt. Follow the [qualification design](../architecture/qualification-release-design.md#result-and-attempt-records).

Update task state only when the [ledger maintenance contract](../plan/20-traceability.md#maintaining-the-plan) is satisfied. Preparing documents does not complete implementation, and owner sign-off cannot be inferred from a tool run.
