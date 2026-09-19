# P1 retained canonical host

`vcp-lifecycle::foundation::CanonicalHost` connects the retained Codex controller
to the [typed foundation](p1-foundation.md), [budget and history](p1-accounting-history.md).
The existing controller owns scheduling, child threads and interruption. One
bounded worker serializes canonical commands, capture and accounting under the
store's real owner lock. The host uses the volatile lifecycle adapter, so the
earlier P0 journal and flat-price ledger do not become a second canonical store.
Retained Codex working history is runtime material; acknowledged VCP facts and
complete captures live in the canonical store.

This is the private host/library boundary required by P1. P2 still implements
OpenRouter capability and price acquisition, production execution policy, tool
brokering and context reconstruction; P3 owns the usable CLI. No public API,
TypeScript SDK, live-provider support or final release package is introduced.

## Source and qualification

| Source | Responsibility |
|---|---|
| `src/crates/vcp-lifecycle/src/foundation.rs` | Trusted thread bindings, canonical owner lifetime, current-authority commands, resume and retained admission traits |
| `src/crates/vcp-lifecycle/src/foundation/worker.rs` | Serialized canonical state, complete capture, checked admission, immutable usage observations and reopen reconciliation |
| `src/crates/vcp-lifecycle/src/foundation/process.rs` | Prepared native operation digest, durable effect states, full channel descriptors and unknown outcomes |
| `src/crates/vcp-lifecycle/src/process.rs` | Existing contained native process path; full-byte observers run before display truncation |
| Retained `core/src/client.rs` and `client_common.rs` | Each actual HTTP attempt receives host admission; provider completion passes response identity and usage |
| Retained `codex-api/src/endpoint/responses.rs` | Exact admitted JSON body; observed response bytes captured before SSE parsing |
| `src/crates/vcp-lifecycle/tests/canonical_host.rs` | Independent HTTP arrival, native output, capacity failure, child/compaction race and fresh-process history fixtures |

Run on native Windows with the documented Rust 1.98.0/MSVC dependencies:

```powershell
pwsh -NoProfile -File scripts/test-p1.ps1
pwsh -NoProfile -File scripts/test-integration.ps1
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode LifecycleTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

The P1 runner requires all 70 named contracts across seven packages: 39 foundation,
accounting and history contracts, 27 retained P0 regressions and four canonical
host integration contracts. It records stable input hashes, actual commands,
toolchain/filesystem identity and logs in ignored `artifacts/p1/<run>/`.
The additional integration runner covers the retained native containment
regressions and seven-request private coding trace. Recovery and lifecycle modes
retain their independent process, filesystem and transport observations.

## Actual request boundary

Host thread registration binds a retained thread to an existing canonical task,
agent and shared root. Unknown threads cannot start work. Turn and continuation
admission check current canonical task/ancestor state as well as the retained
owner. Moving a binding or changing authority requires explicit host rebinding.

For every HTTP attempt, the host checks the selected model against its explicit
price/capability snapshot, inserts `max_output_tokens`, enforces a text input byte
ceiling and rejects unqualified media/provider-tool categories. It captures the
exact final request body, reserves checked conservative input/cache/output bounds
and records send intent before transport. Credentials remain separate header
inputs. The arrival observer independently verifies that the matching submitted
attempt already exists when the scripted provider receives each request.

Hosted requests use the qualified HTTP path. Hidden transport retries are disabled;
an upstream retry must enter admission again. Compaction is attributed from trusted
retained metadata. Helpers and children retain explicit task/root bindings and the
same ledger. Remote memory inference remains denied pending its governed local
adapter. The subsequent [P2 coding host](p2-canonical-coding-loop.md) adds prepared
model-requested tools. The private native operation API also requires host
authority and records the exact operation
digest. A child fixture explicitly carries the same permitted tool exposure as
its parent; default upstream tools cannot grant themselves VCP authority.

The P1 price/capability snapshots and input ceilings are trusted host inputs, not
a live pricing catalog or a claim of tokenizer/provider conformance. Fixtures use
synthetic per-request prices. P2 owns acquisition, context limits and provider
normalization beyond this checked adapter. Conservative cache bounds intentionally
overreserve possible partitions rather than silently assume an unknown rate is zero.

## Capture, failure and reopening

Raw observed response bytes pass through durable capture before SSE parsing or
UI delivery. The response schema explicitly means bytes observed through the
provider terminal event; it does not claim unseen HTTP trailing bytes. Non-success
HTTP response bodies exposed by the retained transport are captured as aborted
observations. Provider completion preserves its request identity and usage with
the captured raw evidence. Missing usage retains an unresolved liability.

Native stdout and stderr stream independently through bounded buffers before the
display limit. The qualification retrieves 2 MiB from each channel despite a
1 KiB display limit. Process execution and task completion remain distinct:
an exit code updates an effect and never substitutes for applicable verification.
Dropping an owned process records an unknown outcome and terminates its native job.

The canonical worker accepts at most 32 queued operations and waits at most
30 seconds for a receipt. An uncertain operation fences subsequent dispatch;
inspection and late reconciliation remain available. Capture capacity is explicit,
positive and at most the foundation's 1 GiB limit. Failure pauses canonical work,
stops dependent transport/process output and preserves the retained prefix.
The capacity fixture exercises real spool writes; it is not a physical disk-full
or hardware power-loss experiment.

Closing or dropping the owning host holds canonical tasks and seals the retained
owner. Reopen converts possible sends and interrupted effects into reconciliation
states, preserves charges and starts tasks paused. Deliberate resume checks the
current binding, owner, repository fingerprint, budget and unresolved effects;
saved execution frames never authorize replay. The fixture proves that no request
arrives before explicit resume and that earlier spend remains in the root ledger.

The history fixture combines an actual interrupted native process, a paused child
and a late model charge. It drops only projections and rebuilds in a separate
process at the same watermark. All facts and final sequences agree, provider
request count stays unchanged and the process marker is not recreated.

See [P1 qualification](../evaluations/p1-completion.md) for exact evidence and
the tested durability envelope. Later production provider, policy, context and
release tasks retain their own acceptance gates.
