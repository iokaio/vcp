# P0 qualification handoff

The feasibility gate selects a cohesive Rust/Codex foundation and bounded native
adapters. It does not declare VCP's first usable release complete. The
[task ledger](../plan/20-traceability.md) remains authoritative for task states;
[the P0 dossier](../evaluations/p0-06-handoff.md) binds the acceptance evidence.

## Reproduce the engineering baseline

Start with [native source setup](codex-source.md), including long paths, native
Rust/MSVC, PowerShell 7 and Node 24. Ordinary builds use committed source; they
do not fetch upstream or apply patches. `scripts/build.ps1` builds the retained
CLI. The product executable/installer remains P3/P8 work.

Rust 1.98.0 is the combined engineering candidate for the integration,
storage and local-memory prototypes. The ordinary upstream build still defaults
to its original Rust 1.95.0 pin. Reproduce the explicit common-compiler checks
without editing retained toolchain metadata:

```powershell
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode LifecycleTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode Build -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
```

These commands reuse one local cache. The dossier records the exact results;
they do not qualify a supported release compiler for an unbuilt product.

Use the local fast suite before publishing. Select native checks by the changed
boundary, using these implemented entry points:

| Boundary | Command or setup guide |
|---|---|
| Source, links, task graph and delivery harness | `pwsh -NoProfile -File scripts/test.ps1 -Suite fast` |
| Retained native CLI and policy/patch libraries | `scripts/build.ps1`; `scripts/build.ps1 -Mode BoundaryTests` |
| Controller, review, mailbox and continuation | `scripts/build.ps1 -Mode LifecycleTests` |
| Private CLI pause/reopen and native containment | `scripts/build.ps1 -Mode RecoveryTests`; [recovery guide](lifecycle-recovery.md) |
| Integrated coding/receipt/budget/memory trace | `scripts/test-integration.ps1`; [integration guide](p0-integration.md) |
| Real offline CPU embeddings | `scripts/test-embeddings.ps1`; acquire pinned public assets explicitly via [embedding setup](local-embeddings.md) |
| Governed local corpus, indexes and resource scales | `scripts/test-local-memory.ps1`; [resource procedure](local-memory-resources.md) |
| Storage, encryption, independent age tool and handoff fixture | `scripts/test-storage.ps1`; [storage procedure](portable-storage-spike.md) |
| Gemini semantic comparisons | `scripts/upstream/compare-gemini-ports.cjs`; pinned external Node checkout from [Gemini setup](gemini-baseline.md) |
| Codex maintenance reconstruction | `scripts/upstream/reconstruct.cjs` and `scripts/upstream/rehearse-codex-fix.cjs`; [integration instructions](p0-integration.md) |

Models, raw evidence, recovery material and build output remain outside tracked
source. Pinned external assets/tools are prerequisites, not implicit downloads.
Hosted CI stays the fast standard Ubuntu confirmation layer; deliberate
second-machine or release checks are the only reason to dispatch heavy Windows
qualification. Earlier larger-runner results are historical evidence, not current
runner instructions.

## Retained and replaced responsibilities

All Codex paths below are relative to `src/third_party/codex/codex-rs/`.
The machine-readable [effect map](../../src/third_party/components/codex-boundaries.json)
classifies the wider selected closure. The smaller table explains the working
integration rather than proposing a second engine.

| Responsibility | Retained code | VCP candidate and next owner |
|---|---|---|
| C01 lifecycle/protocol | `core/src/thread_manager.rs`, `core/src/session/`, internal protocol and cancellation; app-server protocol/schema machinery remains retained | `vcp-lifecycle` owns host registration, holds, exclusive journal and dispatch authority around the retained loop. P1/P2 replace prototype state/transactions; public transport remains deferred. |
| C02 patch representation | `apply-patch` parser and change types | `WorkspaceTools` parses a bounded whole-file update and revalidates the exact prepared bytes under a file lock. P2-04 generalizes preparation, commit and partial-effect receipts. |
| C03 command policy | Retained `execpolicy` parser/matcher and its baseline tests | The P0 native verifier has fixed executable/argv authority, with a separate host tool-name ceiling. P2-03 must map general matcher outcomes and provenance into host grants; a matcher alone is not authority. |
| C04 native execution | `utils/pty/src/win/job.rs`, process/PTY and Windows sandbox libraries | `vcp-lifecycle/src/process.rs` owns non-breakaway jobs and bounded capture; P0-05 separately qualifies AppContainer canaries. P2/P8 compose and qualify the real tool sandbox. |
| C05 agent control/context | `core/src/client.rs`, `client_common.rs`, `tools/registry.rs`, context/prompt libraries, turn interruption and rollout types | Explicit fixture provider, shared budget/usage receipts, prepared tools, governed memory and root/child pause. Helpers retain host gates or are disabled; rollout cannot grant new authority. P2-01/02/07/08 and P3-04 own production context, provider and recovery integration. |
| C06 CLI | Retained `cli/`, `exec/`, `tui/` and configuration parsing | Baseline still identifies as Codex; private `lifecycle-owner`/`integration-owner` qualify host seams. P3 builds the VCP experience using those retained components. |
| C07 tests | `core/tests/common`, retained turn/patch/policy/process tests | VCP controller/integration/port contracts and independent native process/cryptographic observers retain their acceptance meaning. |
| Supporting credential/network libraries | `login`, provider/client primitives and adapters | Retain useful primitives; the host profile supplies only explicit synthetic credentials and disables ambient discovery, telemetry, optional helpers and update/bootstrap routes. Ordinary upstream CLI behavior is not a qualified VCP entry point. |
| Local memory/search | `src/third_party/munarium/server/src/{munarium-core,munarium-store-mem,munarium-datastore}` | Reuse kernel gates and Tantivy/DiskANN. In-memory stores/indexes are projections or qualification fixtures, never a competing authority. P5 owns durable production governance and generations. |

## Concrete segment 02 starting points

The following are implementation destinations, not already implemented packages.
Register each useful package in the retained Cargo workspace when its first
working slice lands. Keep new domain/protocol code free of UI, network clients and
concrete stores.

| Task | Files to create/edit and evidence to preserve |
|---|---|
| P1-01 | Create `src/crates/vcp-domain/src/{ids,workspace,task,revision,verification}.rs`; translate at `vcp-lifecycle/src/lib.rs` and retained `core/src/session/` seams. Replace prototype counters with scoped typed domains without changing retained scheduling ownership. |
| P1-02 | Create internal `src/crates/vcp-protocol/src/{command,event,content,version}.rs`; adapt `vcp-lifecycle/src/control.rs`. Preserve stable command IDs, cached acknowledgements, stale-revision rejection and pause semantics. Public API remains deferred. |
| P1-03 | Create `src/crates/vcp-store/src/{artifact,capture,integrity}.rs`; use P0 storage/effect experiments as contract inputs. Retain full provider/tool evidence and explicit truncation/unknown states. |
| P1-06 | Create `src/crates/vcp-audit/src/{projection,cursor,inspection}.rs` after P1-04. Consume committed events/receipts instead of treating Codex rollout or memory caches as another writer. |

The adjacent storage/budget segment creates `vcp-store` canonical transactions
and `vcp-budget` monetary accounting. Port the behavioral contracts from
`vcp-storage-spike` and `vcp-lifecycle`, but do not promote their full-view JSON,
private journal or flat synthetic tariff to supported production formats.
Preserve the original evidence and compatibility fixtures during migration.

The next coherent PR should combine the P1 domain/internal-protocol/capture slice
with the storage prerequisites that make it useful, following the ledger's
dependency order. Do not publish one PR per small interface. Run local contract,
recovery and integration checks before using GitHub CI as confirmation.
