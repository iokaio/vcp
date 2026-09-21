# P6-03 / M2 — Caller-owned advisory scheduling lease

Status: implemented and locally verified, September 20, 2026. This is the second
M2 increment. It performs no gateway request and changes no escalation action.

## Behavior

The [selected lease](../adr/037-caller-owned-advisory-scheduling-lease.md) creates
one revisioned schedule per canonical advisory request. Exact scheduling repeats
are read-only. Only a newly updated pending lease can produce a claim; every later
claim attempt observes existing state and cannot authorize replay.

The caller must revalidate immediately before transport. Pause, other non-running
states, input changes and deadline expiry close the lease. Caller cancellation and
interruption are explicit. Reopen preserves a claimed lease without treating it as
permission to send. Only an accepted-current canonical result can complete it.

## Verification record

The Files and SQLite fixture verifies schedule deduplication, a single claim,
read-only duplicate claims, claimed-state reopen without replay, explicit
interruption, pause between claim and dispatch, a successful dispatch recheck and
completion through the matching result.

- `cargo check --manifest-path src/crates/vcp-lifecycle/Cargo.toml`: **passed**.
- `cargo test --manifest-path src/crates/vcp-lifecycle/Cargo.toml --test routing_state -- --test-threads=1`: **22 passed**.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 cases passed**, manifest `d2de3d65-8541-461b-a343-a488e3faef26`.
- Repository/link contracts, focused `rustfmt --check` and `git diff --check`: **passed**.

Existing lifecycle/upstream compiler warnings remain. No live provider, paid call
or packaged qualification was run.

## Remaining

M2 remains in progress. Ordinary helper reservation/submission/settlement and raw
response capture are next. Exact-cycle evidence and the local statistical shadow
producer remain behind that lifecycle integration.
