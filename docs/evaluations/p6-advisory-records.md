# P6-03 / M2 — Canonical escalation-advisory records

Status: implemented and locally verified, September 20, 2026. This is the first
M2 increment. It enables no scheduling, transport, advice consumption or provider
qualification.

## Behavior

The [selected boundary](../adr/036-canonical-escalation-advisory-records.md)
persists an exact prepared escalation-advisory request before scheduling and one
decoded result afterward. Request identity derives from its qualified transport
commitment. Exact repeats are read-only and conflicting request or result reuse is
rejected.

Canonical workspace/task state revalidates scope, root, task and steering revisions,
authority and deletion epoch. Results retain a closed current-or-historical
disposition. Changed input and expired requests therefore remain inspectable but
cannot masquerade as current advice.

## Verification record

The focused lifecycle fixture runs on Files and SQLite. It verifies exact request
and result deduplication without watermark changes, current acceptance, changed
policy binding as historical evidence, conflicting-result rejection, stale request
rejection, task-scope denial and reopen parity.

- `cargo check --manifest-path src/crates/vcp-lifecycle/Cargo.toml`: **passed**.
- `cargo test --manifest-path src/crates/vcp-lifecycle/Cargo.toml --test routing_state -- --test-threads=1`: **21 passed**, including the new fixture across Files and SQLite.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 cases passed**, manifest `632b1191-7443-4515-a97f-f49a022b1376`.
- Repository/link contracts and `git diff --check`: **passed**.

Existing lifecycle/upstream compiler warnings remain. No live provider, paid call
or packaged qualification was run.

## Remaining

M2 remains in progress. Caller-owned async scheduling, pause/cancel behavior,
ordinary remote accounting and recovery/reopen scheduling tests are next. Exact
cycle facts and the separate local statistical shadow producer follow that gap.
