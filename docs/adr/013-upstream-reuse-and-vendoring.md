# ADR-013 — Upstream reuse and committed vendoring

Status: repository policy implemented for the initial Codex source selection; VCP integration qualification remains planned.

This record makes the source-management convention explicit for the existing maximum-reuse direction in [architecture section 0.2](../architecture/vcp-what.md#02-open-source-reuse-policy). It does not mark P0 or an upstream import complete. [P0-07 and P0-08](../plan/01-upstream-feasibility.md) will add the exact revisions, retained modules, replacements, dependency closure, and native Windows evidence. P8-06 owns the release-stage update rehearsal.

## Decision

VCP's engine and CLI will be derived from a pinned Codex Rust baseline with maximum reasonable reuse. Preserve working control flow, terminal behavior, execution primitives, supporting libraries, and relevant tests when they can satisfy VCP's contracts. Extract or replace components where coupling obstructs those contracts. Logical `vcp-*` responsibility names do not require rewriting every component into a new crate.

Selected Codex source will be copied into `src/third_party/codex/` and committed as ordinary files in VCP's Git history. Preserve upstream-relative paths where practical, including the `codex-rs/` workspace structure when retained. P0 records the exact build manifest and path map. The selected source is part of VCP's authoritative build graph, not an external reference checkout.

This plan uses neither a Git submodule nor a Git subtree workflow. Do not add a gitlink, nested `.git` metadata, or a required upstream fork. An isolated checkout may be used to inspect, build, or reconstruct an upstream revision during maintenance; it is not a runtime or ordinary VCP-build prerequisite.

The committed tree contains the reviewed, patched source that the build consumes. A normal build must not download Codex source or apply the patch series to that already-patched tree. The provenance manifest and patches are verification and maintenance inputs, not a replacement for committed source. Ordinary package dependencies still follow the qualified toolchain and lockfile setup; this decision does not promise a fully offline build without provisioned dependencies.

## Scope of reuse

The C01–C06 inventory in [architecture section 3.5](../architecture/vcp-what.md#35-codex-as-the-primary-rust-foundation) identifies integration responsibilities, not a cap of six small imports:

| Inventory | Retained or adapted responsibility | VCP logical destination |
|---|---|---|
| C01 | App-server lifecycle and protocol tooling; public API exports remain deferred | `vcp-protocol`, `vcp-engine` |
| C02 | Patch parser and change representation; first direct-extraction candidate | `vcp-tools` |
| C03 | Execution-policy parsing and matching | `vcp-policy` |
| C04 | Process, job, PTY, and applicable sandbox primitives | `vcp-exec` |
| C05 | Agent loop, cancellation, tool dispatch, instruction discovery, and history handling | `vcp-engine`, `vcp-context` |
| C06 | Terminal and headless CLI infrastructure | `vcp-cli` |
| C07 | Relevant upstream regression tests and fixtures | Owning modules and shared contract suites |

Retain suitable runtime, HTTP, serialization, PTY, credential, and terminal libraries after revision-specific dependency and license review. The initial [retained-file list](../../src/third_party/components/codex-files.json) establishes a baseline; the integrated retained/replaced map remains P0-08 work.

VCP supplies or replaces the OpenRouter model gateway, routing, budgets and cost ledger, canonical storage, Munarium-derived local memory with Tantivy/DiskANN, and encrypted portability. Adapters must leave one authoritative controller, store, and ledger. Pointing an upstream provider at another base URL does not establish the required tool-call, attribution, accounting, or recovery behavior.

Remove or disable upstream telemetry and implicit credential discovery from VCP execution paths. Where credential or network functionality is needed, route it through explicit VCP configuration and authority. Useful credential-library primitives may remain. Account for helper, retry, review, and compaction model calls through VCP; do not preserve hidden effects merely to minimize the patch size.

## What is committed

| Artifact | Contract |
|---|---|
| `src/third_party/codex/` | Selected Codex source and dependency closure, with reviewed VCP patches already applied; excludes repository metadata, secrets, and build output |
| `src/third_party/upstreams.toml` | Origin, immutable commit, fetch date, selection/destination map, reuse form, dependency closure, original and resulting content hashes, notice/license paths, patch order, owner, and qualification evidence |
| `src/third_party/patches/codex/` | Ordered, reproducible patches for every local difference from the recorded source selection, with rationale and upstream references where available |
| `src/third_party/licenses/` and retained source notices | Applicable original license and NOTICE texts, plus required modification markers on changed files |
| VCP modules containing attributed ports | Original and destination paths, revision, attribution, and reproducible port/change history linked from the manifest |

The initial manifest schema and reconstruction tooling implement the source records above; qualification evidence remains per component. Record path mappings, exclusions and content normalization explicitly so reconstruction has one unambiguous expected result. Do not invent commit hashes or claim an import before it exists.

Changes to vendored source and its patch records must be reviewed together. A patch may be updated or replaced in the ordered series, but applying the recorded series to the selected pristine revision must reproduce the committed result. Do not maintain an unrecorded second set of edits or apply patches twice during a build.

## Import and qualification workflow

1. P0-07 selects immutable revisions, dependency closure, paths, and rights. Build the selected unmodified Codex baseline on native Windows in an isolated temporary location and record the commands, environment, and any upstream failures.
2. Prepare the selected source as ordinary files in VCP, retaining the cohesive workspace where useful. Add provenance and required notices with the import. Committing an import follows the authorization for that implementation task; this ADR does not perform or authorize a commit in the current documentation task.
3. P0-08 inserts VCP adapters into the functioning baseline. Record retained modules, replacements, exact build entry point, patch size, and integration evidence in the source map and this ADR's follow-up evidence.
4. Verify two paths independently: build VCP from a clean clone containing the committed patched files, and reconstruct those files in a disposable directory from the pinned upstream selection plus ordered patches. Compare hashes, paths, and notices; report any drift as a failure.
5. Run retained upstream checks and affected VCP contracts. P0-08 imports a representative upstream fix to measure maintenance effort; P8-06 repeats an update rehearsal for the integrated release.

P0 must expose the selected build through the planned `scripts/build.ps1` and checks through `scripts/test.ps1`. Import/reconstruction helpers belong under `scripts/upstream/` when implemented and run only as explicit maintenance or verification work. Do not bootstrap an import as an implicit build side effect.

## Upstream updates and attribution

Review relevant fixes before each release and in response to security/compatibility reports. An authorized update selects a new immutable revision, reviews source and transitive dependency changes, reapplies or revises the local patch series, updates the committed source and manifest together, and reruns affected upstream and VCP suites. No build or runtime step automatically advances an upstream revision.

Retain applicable license and notice texts and identify modified files when source is first imported. Update root [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) and [NOTICE](../../NOTICE) with actual included material at import time, then verify the inventory against what ships before release. VCP's Apache-2.0 license does not replace selected-file, dependency, or model-asset terms.

The trade-off is a larger VCP source tree and responsibility for keeping patches and provenance synchronized. In return, a normal clone contains the exact source reviewed for the build, and upstream maintenance can be checked independently of ordinary compilation. A future switch to submodules, subtree merges, or fetch-and-patch-only source delivery requires an explicit revision to this ADR, the layout, and P0 procedures.

## Current evidence and unresolved work

The [current source selection and build](../development/codex-source.md) retain the pinned Codex Cargo workspace as 7,937 ordinary files. Machine-readable selection/result records, reconstruction checks and a native build command exist. The ordered patch series records a crate recursion-limit compatibility change; the separate license-symlink materialization is explicit. Root and retained notices identify the imported source and modification. There is no gitlink, nested repository or `.gitmodules` dependency.

The [Munarium selection](../development/munarium-source.md) imports 70 ordinary
files with manifest-only adaptations into the same Codex Cargo workspace. Its
separate patch series and file inventory preserve attribution and reconstruction.
All 1,491 original Codex lockfile package identities/checksums remain; 34 entries
are added. Component qualification retains the original compiler pins; a single
supported VCP compiler remains an integration decision.

The [static package/effect inventory](../development/codex-boundaries.md) covers all 157 shared-workspace packages and records proposed gates with checked source anchors. It describes intended adapter ownership, not implemented restrictions. Runtime traces, enforced replacements and other selected component/toolchain qualification remain open P0 work; the committed-copy convention is unchanged.
