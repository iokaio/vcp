# Upstream selection and feasibility experiments

Status: P0 procedure with [candidate tooling](upstream-candidates.md) and [Codex source selection, reconstruction and build](codex-source.md) implemented. Product integration and local models remain unqualified. Read [ADR-013](../adr/013-upstream-reuse-and-vendoring.md), [segment 01](../plan/01-upstream-feasibility.md) and [architecture reuse boundaries](../architecture/vcp-what.md#38-boundaries-around-borrowed-components).

## Experiment record

Give each experiment a stable ID tied to P0-01 through P0-09 and its R/E/M/U cases. Record hypothesis, candidate revision, alternative, fixture, expected observations, limits and abort condition before running. An experiment may reject a candidate while still producing useful evidence; distinguish experiment execution from qualifying the candidate.

For Codex, keep baseline and adapted runs separately identifiable. For Gemini ports, retain upstream expected behavior and VCP's intentional differences. For Munarium, verify the selected kernel/local-inference path without assuming its server deployment belongs in VCP.

## Proposed source-selection manifest

The [current Codex schema](codex-source.md#selection-and-retained-structure) implements source selection and hashes. The broader qualification record requires each group below; effect/integration and release evidence remain separate gates:

| Field group | Proposed contents and validation |
|---|---|
| Identity | Component ID, origin URL, immutable commit, selected tree identity, fetch date, lineage; reject branch-only pins |
| Selection | Ordered include/exclude rules, upstream-to-destination map and exact selected-file inventory |
| Content | Pristine and resulting path/byte digests, byte length, file kind, executable metadata when relevant |
| Changes | Ordered patch IDs/digests and port notes; each difference has rationale and owning adapter |
| Closure | Internal package paths, external dependency locks, native libraries, build-time generators and asset inputs |
| Attribution | Actual selected license/NOTICE paths, modification markers, destination notices and asset-specific terms |
| Effects | Network/model/credential/process/filesystem/service behavior and the VCP replacement or restriction |
| Evidence | Native baseline, adapter comparisons, retained tests, unsupported targets and actual commands |
| Maintenance | Responsible project role, upstream review source, update rehearsal and rollback/compatibility constraints |

Choose the digest algorithm and deterministic inventory encoding once in P0-07, using an established implementation. Sort by normalized repository-relative path, detect duplicate/case-colliding paths on Windows, and state whether bytes are normalized. Hash original bytes before any declared transformation. A Git commit alone does not describe a selected or modified subtree.

Do not copy repository metadata, local configuration, credentials or generated build output. File kinds and links require an explicit import policy; resolve their selected dependency closure rather than following arbitrary host links.

## Two independent reproducibility paths

The normal build consumes committed already-patched Codex source under `src/third_party/codex/`. Its dependency provisioning follows the pinned lockfile/setup and is separate from source reconstruction.

The maintenance verifier starts in a distinctly named disposable directory:

1. Verify the immutable upstream object and select precisely the recorded paths.
2. Apply only declared mapping/normalization and the ordered patch series once.
3. Enumerate resulting files and compare path set, kinds, byte lengths and hashes to the proposed committed tree.
4. Verify retained notices and required modification markers; report missing and unexpected paths.
5. Build the reconstructed selection when the experiment needs build equivalence.
6. Preserve failed reconstruction evidence. Clean up only the explicitly created disposable resource after verifying its resolved location.

An update must pass both paths. A clean VCP build can succeed despite missing patch history; reconstruction can succeed despite a broken VCP adapter. Neither result substitutes for the other.

## Native baseline and effect inventory

P0-07 records the actual manifest/build target, compiler, architecture, Windows version, native dependencies and configuration required for the unmodified upstream. Run its relevant retained tests before changing behavior. Record unsupported upstream targets and failures without silently altering expected results.

Trace every entry point reachable from a coding task, helper, compaction, review and shutdown. Classify it as pure, read-only or effectful. The inventory must locate provider calls, retry loops, telemetry, credential discovery, subprocesses, file writes, caches, databases, scheduled maintenance and update checks.

For each effectful path, name the eventual VCP gateway or broker and the observation that proves it uses it. Static search is a discovery aid; a controlled request/effect observer must also detect hidden calls in representative flows. Retaining a credential library is compatible with disabling ambient discovery.

## Comparison protocols

| Experiment | Controlled inputs | Required measurements or oracles |
|---|---|---|
| Local memory | Same scoped synthetic corpus, embedding spec and query truth set | CPU inference, exact-vector recall, reopen IDs, peak resident/mapped memory, network isolation |
| Canonical storage | Same neutral records, transaction schedule, artifacts and faults | Acknowledged record set, idempotency results, ledger totals, restore/conversion equality |
| Encrypted portability | Same logical snapshot/key policy, measured encryption enabled | Ciphertext transfer churn, peak staging disk, restore time, wrong-key/tamper/writer/replay failures |
| Windows execution | Explicit executable/argv/environment and independent marker tree | Actual child/grandchild ownership, termination, path/link behavior, output limits |
| Lifecycle seam | Identical scripted model/tool sequence and close/pause barriers | One controller, no admission after stop, preserved partial effects and costs |
| Gemini port | Same neutral argument/event schedule and original expected result | Matching semantics plus separately declared VCP authority/accounting differences |
| Update rehearsal | Recorded immutable source revision plus representative fix | Changed paths, patch conflict effort, retained regression outcomes and new dependency closure |

Do not compare a plaintext storage snapshot against an encrypted alternative as if their transfer costs were equivalent. Record warm/cold state, repetitions and workload size; a tiny smoke fixture cannot qualify large-corpus performance.

## Qualification dossier

P0-06 assembles a candidate-by-candidate matrix: qualified for the stated envelope, rejected with evidence, or not run. Include the selected source map, unresolved choices, bounded replacement work, setup instructions and operating limitations. Cross-reference [ADR-001](../adr/001-runtime-topology.md), [ADR-003](../adr/003-canonical-storage.md), [ADR-008](../adr/008-local-governed-memory.md), [ADR-015](../adr/015-portability-and-storage-choice.md) and [ADR-019](../adr/019-cloud-encryption-and-keys.md).

A replacement must preserve required capabilities; failing DiskANN, local embeddings or encryption experiments is not permission to quietly remove them. Resolve material product trade-offs explicitly. Add a real source map only after the mapping exists; this guide is a schema and procedure, not an invented import inventory.
