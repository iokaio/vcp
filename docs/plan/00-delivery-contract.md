# 00 — Delivery contract and implementation conventions

Status: planned. Read with [the architecture](../architecture/vcp-what.md) and [test guide](16-test-fixtures-and-acceptance.md). This file supports every work item without creating additional product scope.

## Fixed product requirements

- Owner tests locally first; subsequent downloadable releases use Apache-2.0 with all required third-party notices.
- Native Windows CLI is the first release. Internal separation is required; public API, SDK, VS Code, hooks, foreign configuration import and other platforms are deferred.
- Reuse as much working Codex engine/CLI code as reasonable, selected Gemini tool/policy/extension code or ports, and selected Munarium kernel/local-memory code. One VCP controller, canonical store and cost ledger remain authoritative.
- Coding models use OpenRouter. Embeddings, indexes and the memory engine run locally. Missing local inference produces a setup/degraded state, never a remote embedding fallback.
- Retain full observed code-work history within workspace scope, automatically derive all supported memory classes with evidence labels, notify when history exceeds 30 days, and prune only by user command or saved policy.
- Local working files, SQLite/files state and indexes need no VCP encryption. Every cloud-bound backup object and manifest must be encrypted before publication, using developer-controlled recovery material held separately.
- The CLI owns task lifetime. Closing it pauses root and children; reopening the workspace reconciles and resumes deliberately.
- Routing, `/optimize`, MCP, bundled skills and visible delegation are required for usability. Autonomy, spend, interactivity and OS isolation are separate controls.

## Repository and module boundaries

Use the logical packages in architecture section 3.3 as responsibility names. Segment 01 records where they live in the selected Codex-derived workspace. An existing cohesive upstream module can implement several logical boundaries; avoid a competing engine beside an unchanged upstream engine.

| Logical area | Proposed source responsibility | Dependency rule |
|---|---|---|
| Domain/protocol | `vcp-domain`, internal `vcp-protocol` | No concrete store, network or UI dependencies |
| Engine | `vcp-engine` controller/scheduler | Calls injected model, policy, store, context and execution contracts |
| Persistence | `vcp-store`, artifacts, projections | Acknowledgement follows durable commit; no network call inside transactions |
| Models/routing/budget | `vcp-models`, `vcp-routing`, `vcp-budget` | Every request passes capability filtering and atomic cost admission |
| Tools/execution/policy | `vcp-tools`, `vcp-exec`, `vcp-policy` | No direct-write/process shortcut around prepared authority and receipts |
| Memory/search | `vcp-memory`, Tantivy/DiskANN adapters | Canonical acceptance independent of index visibility; all queries enforce current scope |
| Context/repository/extensions | `vcp-context`, `vcp-repository`, `vcp-extensions` | Attributed inputs cannot grant authority |
| CLI/audit | `vcp-cli`, `vcp-audit` | Read projections and issue internal commands; no second scheduler |
| Portability | Proposed modules under `vcp-store`: `snapshot`, `vault`, `keys`, `restore` | Only the vault publisher writes cloud-bound objects; it accepts finalized ciphertext |

Keep upstream adapters thin and classified as pure computation, read-only I/O or effectful. Record removed telemetry/auth discovery, replaced model helpers and authority differences. Preserve original source/fixture provenance for attributed ports.

## Work-item procedure

Before editing code, record the task ID, completed prerequisites, concrete source paths and governing ADRs. Split implementation by observable behavior rather than by arbitrary file count. A normal change includes the contract/type addition, production path, error behavior, focused regression fixture and an inspector/event consequence where relevant.

At durable boundaries, specify the order of artifact staging, canonical commit, acknowledgement and external dispatch. At authorization boundaries, specify the identity/revision checked and what invalidates it. At destructive cleanup boundaries, name protected dependencies and the expected result after interruption. These are implementation inputs, not details left for a final review.

Do not make migrations, data resets or upstream replacement implicit. Retain user edits and existing state. A prototype can use a disposable store; production conversion requires a recoverable snapshot and explicit compatibility checks.

## Test entry points to implement

P0-01 establishes a small test runner and P0-07 maps it onto actual upstream packages. Proposed entry point:

```powershell
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
pwsh -NoProfile -File scripts/test.ps1 -Suite store -Backend both
pwsh -NoProfile -File scripts/test.ps1 -Suite windows
pwsh -NoProfile -File scripts/test.ps1 -Suite acceptance -Case U04 -Backend both
```

These commands do not exist yet. The runner must reject unknown suites/cases, preserve nonzero exit codes, print the concrete commands it runs and mark skipped prerequisites. Add suites `provider`, `context`, `tools`, `recovery`, `cli`, `memory`, `search`, `retention`, `portability`, `routing`, `extensions`, `delegation`, `distribution` and `upstream` as their implementations land. Build, formatting and lint commands target selected VCP/upstream packages; do not silence existing upstream failures or demand unrelated platform builds.

`fast` runs deterministic unit/contract checks without network or model assets. Native process tests run on Windows, real embedding tests require the pinned local model, and live OpenRouter evaluations require an explicit model policy and spend cap. A missing prerequisite is `not_run`, never `passed`.

## Evidence format and completion

Each run produces a local manifest with task/test IDs, commit and dirty diff identity, upstream/library/model versions, OS/CPU/RAM, configuration/policy/backend revisions, fixture version/seed, actual command lines with secrets excluded, exit codes, duration, artifacts, and limitations. Record failures and retries as attempts. Retain raw results plus a short reviewer summary; do not commit private prompts, recovery keys or owner repositories as fixtures.

Allowed work states are `planned`, `in_progress`, `blocked`, `implemented_unverified` and `complete`. The initial state of every item is planned. Completion requires its tests and dependencies, a documented operational failure path, and evidence tied to the current source revision. A later relevant edit invalidates stale verification.

## Decisions that precede dependent code

| Decision | Work owner | Evidence/output |
|---|---|---|
| Concrete Codex/Munarium seams, compiler and native dependency versions | P0-02, P0-07, P0-08 | Immutable provenance manifest and native Windows build report |
| SQLite/files layout and advertised support | P0-04, P0-06, P1-04 | Same canonical, crash, migration and encrypted-transfer contracts |
| Local model/runtime and supported hardware | P0-02, P5-04, P8-01 | CPU-only inference/reopen benchmark and provisioning/license record |
| Encrypted archive and authorized-writer authentication | P0-04, P0-06 | ADR-019; compatibility, tampering, key recovery and sender-trust fixtures |
| Autonomy defaults and supported sandbox claims | P2-03, P8-01 | Explicit preset/rule matrix plus actual Windows enforcement tests |
| Model pools, dollar/latency/quality defaults | P6-04 | Revalidated catalog and measured total-cost evaluation |
| Owner repositories and acceptance thresholds | P0-01 proposes; P8-05 confirms before final scoring | Versioned private fixture references and predeclared pass criteria |

Engineering experiments resolve implementation choices within confirmed requirements. Seek a new product decision only when an experiment shows a material trade-off the architecture has not authorized; record the result instead of quietly dropping a required capability.
