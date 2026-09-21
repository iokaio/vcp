# P6-03 / M2 — Advisory helper accounting binding

Status: implemented and locally verified, September 20, 2026. This is the third
M2 increment. It performs no gateway request and consumes no advice.

The Files and SQLite fixture creates a canonical advisory request and claim,
reserves an ordinary helper attempt against a retained typed request body, binds it
idempotently, submits it through `vcp-budget`, and retains an uncertain charge.
The advisory lookup reports the canonical submitted and reconciliation-pending
phases and reopens with the same attempt.

The identity follow-up requires the retained artifact digest to match the canonical
serialized `RequestRecord`, as well as the attempt's admitted digest. The quote's
model, provider and capability must match the stored evaluator. Both initial binding
and subsequent reads enforce these checks, so older mismatched bindings fail closed.
This identifies request evidence; the artifact is an envelope, not the gateway wire
body. Finite charge qualification and exact transport-byte validation remain the
runtime capability's responsibility.

Adversarial fixtures cover another valid request in the same task and mismatched
model/provider/capability identities on Files and SQLite, including stored legacy
bindings. Rejected binds do not mutate canonical state.

Identity follow-up verification: **25 routing-state tests passed** and **9 fast
cases passed**, manifest `62053a40-16d1-4af1-bfe8-6f5f3007215d`. Focused formatting
and diff checks passed. Initial construction evidence follows.

- `cargo check --manifest-path src/crates/vcp-lifecycle/Cargo.toml`: **passed**.
- `cargo test --manifest-path src/crates/vcp-lifecycle/Cargo.toml --test routing_state -- --test-threads=1`: **23 passed**.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 cases passed**, manifest `697de384-772c-46e4-9e8b-5e211d9c14de`.
- Repository/link contracts, focused `rustfmt --check` and `git diff --check`: **passed**.

Existing lifecycle/upstream compiler warnings remain. No live provider, paid call
or packaged qualification was run.

Remaining work connects the caller-owned send/response path and decoded canonical
result to this attempt. Exact-cycle and local statistical shadow work still follow
that lifecycle integration.
