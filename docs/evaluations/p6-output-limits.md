# P6-02/P6-05 — Selected output limits

The optimizer can preview and apply a selected maximum output-token count, or
restore inheritance with `--output-tokens inherit`. Persisted preferences remain
separate from their effective value under the trusted startup and routing
ceilings. Requests never exceed either ceiling. An absent optional policy field
retains the historical JSON representation and policy digest.

The effective output limit flows through candidate encoding, routing input,
canonical context sealing, the captured provider body and the reservation quote.
Admission copies its output bound from the validated sealed request; changing
policy does not rewrite an already dispatched request or its charge evidence.
Current policy identity is still checked at admission and the send boundary.
Explicit contexts without a routed decision keep their trusted host output
configuration. Evaluator request limits remain independently qualified.

## Verification

On native Windows, these commands passed with the repository's provisioned
stable Rust environment and `artifacts/codex-target`:

```powershell
cargo +stable test --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline -p vcp-models --test routing -- --test-threads=1
cargo +stable test --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline -j 4 -p vcp-lifecycle --features qualification --test canonical_host routing_output:: -- --test-threads=1
```

All **17 routing tests** passed. Added coverage checks absent-field byte/digest
round-trip, rejection of zero output, pure-selector enforcement, and rejection of
a prepared routing decision after a selected-output policy change.

The **one native output test** passed four scenarios across SQLite and Files:

- A selected limit of 256 is observed in the actual controlled-peer HTTP body,
  routing input and durable reservation.
- Changing the selected limit to 64 after that request reaches the peer preserves
  its original 256-token reservation and settled charge.
- A persisted selection of 4096 is clamped to the startup ceiling of 1024 in the
  actual body, routing input and reservation on each backend.

The host log is `artifacts/p6-selected-output-host.log`. Existing workspace
warnings did not fail the run. These are targeted development checks, not a
frozen live profile-qualification campaign.

## Limits

The peer, prices and model-quality labels are synthetic. No paid provider call,
actual model-quality improvement, or shipping profile default is established.
The stale-policy regression exercises deterministic decision revalidation; the
native test changes policy after dispatch and does not inject a policy change
between reservation and network transmission. Input/context selection, provider
effort, retrieval defaults, and delegation concurrency are separate acceptance
work; this increment changes output limits only.
