# P1 accounting and history

The second P1 increment adds `src/crates/vcp-budget` and `src/crates/vcp-audit`
to the retained Cargo workspace. Both use the [canonical foundation](p1-foundation.md).
They do not send model requests or start a second controller. The
[retained canonical host](p1-retained-host.md) connects actual transport attempts
and native effects to these libraries; [P1 qualification](../evaluations/p1-completion.md)
records the integrated acceptance evidence.

## Qualification

```powershell
pwsh -NoProfile -File scripts/test-accounting-history.ps1
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

The native runner executes 39 named contracts across six packages using locked,
offline Rust 1.98.0 and MSVC dependencies. It records source hashes, versions,
commands, outcomes and logs in ignored `artifacts/accounting-history/<run>/`.
Fixtures use synthetic prices and artifacts; no private credentials or paid
provider are required. Process fixtures require the packages' `qualification`
features and exercise both SQLite and files backends.

## Accounting contract

Money and usage are checked integer domains serialized as decimal strings.
Each quote records currency, explicit price and capability identities, expiry,
usage ceilings and normalization version. Every supported category needs an
explicit rate, including zero. Division rounds up; overflow rejects admission.
Cache read/write units partition input; reasoning is a subset of output and is
not billed twice. These are normalization rules, not production price claims.

Admission atomically commits the complete captured request, reservation, attempt,
root ledger and event. Callers can compose a task revision and command receipt in
the same transaction. Optimistic revisions make concurrent admissions recalculate
after a conflict. The root counts settled charges, active liabilities, unresolved
liabilities and protected verification funds. Child allocations subdivide that
same root. A verification draw replaces protected funds with a reservation;
releasing an unsent draw returns them once. After submission, an unused allowance
is not automatically placed back in the protected reserve; explicit policy can
allocate it again. Canonical validation independently checks aggregate totals,
retry ancestry, child/daily ceilings, running ancestors and protected draws.

`submit` records send intent durably before returning a non-serializable send
capability. Repeating it cannot mint another capability. A lost response keeps
the liability in `reconciliation_pending`; a retry has a new identity and records
its predecessor. Reopen and backend conversion retain both liabilities.
Reducing a cap preserves existing debt. Actual late charges are recorded even
when they exceed a cap, and the overrun blocks further unaffordable work.

Usage observations retain raw evidence and immutable adjustments. Observation
IDs are idempotent. Cumulative versions apply monotonically; incremental ranges
must be contiguous and nonoverlapping. A negative correction or explicit uncertain
resolution records actor, policy and reason. Later actual usage can settle an
explicitly resolved estimate. Provider request identity and currency cannot
silently change. Local CPU, RAM and disk observations use a separate collection
and never masquerade as provider spend.

Optional daily policy is explicitly scoped to one local root and a fixed UTC
offset. Previous-day open liabilities still count after midnight. It is not a
global cross-root or synchronized account cap, and it does not infer daylight
saving rules. P2 owns provider bounds and trusted price acquisition.

## History contract

The projector folds canonical events into tasks, effects, accounting and workspace
bindings. Version 2 adds diagnostic event counts; activation compares authoritative
facts with version 1 at the same input watermark. Projection contents, active
version and input watermark publish in one transaction. Rebuilding reads history
without replaying tools or model requests. Unknown effects remain unknown, paused
children remain paused and late charges remain visible.

History supports session, task, agent, provider, model, path, event-kind and
inclusive-start/exclusive-end time filters. Optional event metadata is omitted
when absent, preserving existing version 1 event serialization. A history reader
holds at most four pinned snapshots, each for 60 seconds; pages return at most
128 events and scan at most 256 inputs. The exclusive continuation offset also
advances over filtered inputs. Cursors bind workspace, filter, access scope,
captured end, authority and deletion revisions. Expiry or changed scope returns
an explicit restart result.

Current access and retention masks apply before content filtering or artifact
dereference, including reads from old snapshots. Removed ranges return explicit
gaps without revealing removed payloads. The reader consumes canonical masks;
the P5 retention owner still implements user previews, pruning policy and physical
deletion. This increment does not authorize automatic pruning.

The [increment report](../evaluations/p1-accounting-history.md) preserves its
library evidence. [P1 qualification](../evaluations/p1-completion.md) adds the
retained-controller acceptance and current source identities.
