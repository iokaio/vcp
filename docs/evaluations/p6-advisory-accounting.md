# P6-03 / M2 — Advisory helper accounting binding

Status: implemented and locally verified, September 20, 2026. This is the third
M2 increment. It performs no gateway request and consumes no advice.

The Files and SQLite fixture creates a canonical advisory request and claim,
reserves an ordinary helper attempt against a retained typed request body, binds it
idempotently, submits it through `vcp-budget`, and retains an uncertain charge.
The advisory lookup reports the canonical submitted and reconciliation-pending
phases and reopens with the same attempt.

- `cargo check --manifest-path src/crates/vcp-lifecycle/Cargo.toml`: **passed**.
- `cargo test --manifest-path src/crates/vcp-lifecycle/Cargo.toml --test routing_state -- --test-threads=1`: **23 passed**.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 cases passed**, manifest `697de384-772c-46e4-9e8b-5e211d9c14de`.
- Repository/link contracts, focused `rustfmt --check` and `git diff --check`: **passed**.

Existing lifecycle/upstream compiler warnings remain. No live provider, paid call
or packaged qualification was run.

Remaining work connects the caller-owned send/response path and decoded canonical
result to this attempt. Exact-cycle and local statistical shadow work still follow
that lifecycle integration.
