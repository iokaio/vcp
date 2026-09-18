# VCP implementation and testing plan

Plan revision 4 — September 17, 2026. Status: planned; every segment expanded with implementation contracts, construction sequences, failure handling and supporting design/ADR references. Explicit pause while the CLI stays open is required alongside close-to-pause. No implementation or runtime test result is implied.

Start with [the code layout](code-layout.md), [the delivery contract](00-delivery-contract.md), [test infrastructure and acceptance](16-test-fixtures-and-acceptance.md), then [upstream feasibility](01-upstream-feasibility.md). These files expand [architecture draft 0.4](../architecture/vcp-what.md) into coding and testing work. The architecture remains authoritative for product behavior; this directory owns the detailed execution instructions and repository layout.

The repository contains the executable [delivery harness](../development/delivery-harness.md), [committed Codex baseline](../development/codex-source.md) and [checked package/source map](../development/codex-boundaries.md), with repository and regression checks in CI. P0-01 and P0-07 are complete; P0-02's [local corpus experiment](../development/local-memory-spike.md) is in progress. The [selection acceptance map](../evaluations/p0-07-selection-gate.md) separates source qualification from later product tasks. Remaining VCP product interfaces and acceptance results are planned. Use `docs/` for documentation, `src/` for source and test assets, and `scripts/` for build/test automation. Preserve useful Codex modules instead of creating empty replacement crates merely to match a diagram. Follow [CONTRIBUTING.md](../../CONTRIBUTING.md) for licensing, sign-off, provenance, and review.

## Segment index

| File | Deliverable | Architecture work owned |
|---|---|---|
| [Code layout](code-layout.md) | Initial roots, target tree, logical package locations and placement rules | Shared repository convention; no additional product work item |
| [ADR-013: committed vendoring](../adr/013-upstream-reuse-and-vendoring.md) | Codex copied-source policy, build participation, patch reconstruction and updates | Repository policy for P0-07/P0-08/P8-06; component qualification remains planned |
| [00 Delivery contract](00-delivery-contract.md) | Shared working rules, source boundaries, check commands and evidence | Supporting guidance |
| [01 Upstream feasibility](01-upstream-feasibility.md) | Native Windows Codex baseline, Munarium/search/storage/crypto prototypes | P0-01 through P0-09 |
| [02 Engine state and capture](02-engine-state-and-capture.md) | Domain state, internal events, full artifacts and projections | P1-01, P1-02, P1-03, P1-06 |
| [03 Storage and budget](03-storage-and-budget.md) | Canonical backend parity and atomic cost accounting | P1-04, P1-05 |
| [04 Context and instructions](04-context-and-instructions.md) | Workspace identity, AGENTS.md, versioned context and compaction | P2-01, P2-08 |
| [05 OpenRouter and session loop](05-openrouter-and-session-loop.md) | Normalized provider requests, coding loop and completion | P2-02, P2-05, P2-06 |
| [06 Windows tools and recovery](06-windows-tools-and-recovery.md) | Autonomy, prepared edits, process control and close-to-pause | P2-03, P2-04, P2-07 |
| [07 CLI and inspection](07-cli-and-inspection.md) | Interactive/JSONL CLI, inspectors and workspace continuation | P3-01 through P3-04 |
| [08 Memory and ingestion](08-memory-and-ingestion.md) | Munarium-derived claims, evidence and automatic scoped memory | P5-01, P5-02 |
| [09 Local search and generations](09-local-search-and-generations.md) | Tantivy, local embeddings, DiskANN and coherent retrieval | P5-03 through P5-06 |
| [10 History and pruning](10-history-and-pruning.md) | Full-history browsing, 30-day notices and precise pruning | P5-07, P3-05 |
| [11 Encrypted portability](11-encrypted-portability.md) | Developer keys, encrypted snapshots, restore and handoff | P5-09, P5-10, P3-06 |
| [12 Routing and optimization](12-routing-and-optimization.md) | Model groups, profiles, escalation and `/optimize` | P6-01 through P6-05 |
| [13 Skills and MCP](13-skills-and-mcp.md) | Bundled development skills and brokered MCP integration | P7-01 through P7-03 |
| [14 Visible delegation](14-visible-delegation.md) | Child task graphs, isolated changes, integration and commentary | P7-04 through P7-06 |
| [15 Integration and release](15-integration-and-release.md) | Memory qualification, Windows acceptance and downloadable release | P5-08, P8-01 through P8-06 |
| [16 Test fixtures and acceptance](16-test-fixtures-and-acceptance.md) | Deterministic harness, fault cases, U01–U09 and evaluation protocol | Shared testing guidance |
| [17 Deferred API and SDK](17-deferred-api-and-sdk.md) | Public protocol, local attachment and TypeScript SDK | P9-01 through P9-03 |
| [18 Deferred VS Code](18-deferred-vscode.md) | Editor context, changes, inspection and packaging | P4-01 through P4-05 |
| [19 Deferred extensions and platforms](19-deferred-extensions-and-platforms.md) | Hooks, configuration import, observers and later execution hosts | P10-01 through P10-04 |
| [20 Traceability](20-traceability.md) | All 68 items, exact dependencies, requirements and test ownership | Navigation and coverage ledger |

## Execution order

Segment numbers organize reading; individual task dependencies determine execution. A segment may contain an early implementation task and a later integration task. In particular, P1-06 follows storage in segment 03, P2-08 follows the loop in segment 05, and P6-04 follows memory qualification P5-08 in segment 15. Do not turn whole-file completion into an additional prerequisite.

| Wave | Work that can become ready | Gate before moving dependent work forward |
|---|---|---|
| 1 | P0 requirements, source pins, Windows/reuse/local-memory and encrypted-storage experiments | P0-06 records qualified choices and bounded replacements for failed candidates |
| 2 | P1 state/capture, then concrete stores, budget and projections | Durable records, artifact references and reservations pass both advertised backend contracts |
| 3 | P2 repository/model/policy/tools; P5 governance/ingestion; P7 skill discovery | All effects have authority and durability boundaries; no hidden provider calls |
| 4 | P2 loop/verification/recovery/continuity; P3 basic CLI and inspectors; P7 bundled skills/MCP | Internal Windows coding slice is usable for engineering, with incomplete release features labelled |
| 5 | P5 search/retrieval/pruning/portability; P3 history and vault controls; P5-08 memory evidence | U04/U05/U09 subsystem evidence and reproducible local recall |
| 6 | P6 routing/optimizer; P7 delegation/integration/progress | These can start after their exact dependencies, before every wave-5 task finishes; qualified profiles follow P5-08 |
| 7 | P8 integrated owner acceptance, recovery, packaging and maintenance | All 56 first-release work items and U01–U09 complete together |
| Later | P9 then P4; P10 as separately prioritized | The 12 deferred items do not block the Windows CLI |

The first usable release includes coding, automatic selection across groups, persistent local memory, MCP, skills, visible delegation, optimization, full history/pruning, pause/resume and encrypted portable handoff. A fixed-model loop alone is an internal milestone.

## How to use one segment

1. Find the next uncompleted work item whose dependencies are satisfied in [the ledger](20-traceability.md).
2. Read its source contracts and the applicable fixture definitions. Record any unresolved engineering decision before coding against it.
3. Implement the listed increments in reviewable changes, retaining upstream tests and adding VCP boundary tests alongside the behavior.
4. Run the relevant deterministic and Windows integration suites. Run paid evaluations only with an explicit configured evaluation budget.
5. Record actual commands, versions, exit status and artifact locations. Mark the item complete only when its exit criteria pass; describe partial results explicitly.

Do not mark whole segments done from this document's existence. Keep decision records in `docs/adr/`, reviewed redacted summaries in `docs/evaluations/`, and raw run evidence under ignored local artifact roots such as `artifacts/`. Create these locations when their owning work produces content; the layout and community files do not complete any product work item.

## Implementation reading map

Use a task's existing heading as its implementation entry point. Its expanded instructions specify proposed records, ordering, failure behavior and observable tests. Follow the supporting design for cross-service contracts, then the ADR for decisions that still need evidence. Concrete type names are proposals until the actual P0 workspace mapping and owning implementation select them.

| Work | Supporting design | Key decisions |
|---|---|---|
| Foundation and source selection | [Upstream qualification](../development/upstream-qualification.md) | ADR-001, 003, 008, 013, 015, 019 |
| State, storage, budgets, tools and CLI | [Engine and execution](../architecture/engine-execution-design.md) | ADR-001–005, 009, 016 |
| Context, model loop and verification | [Context and provider](../architecture/context-provider-design.md) | ADR-006, 009 |
| Memory, search and retention | [Memory and retrieval](../architecture/memory-retrieval-design.md) | ADR-008, 016 |
| Snapshots, keys and transfer | [Storage and portability](../architecture/storage-portability-design.md) | ADR-003, 015, 019 |
| Routing, skills, MCP and children | [Routing and extensions](../architecture/routing-extensions-design.md) | ADR-006, 007, 010, 011, 017 |
| Harness, evaluations and distribution | [Qualification and release](../architecture/qualification-release-design.md) | ADR-012, 018 |
| Later API, SDK, editor and hosts | [Deferred clients](../architecture/deferred-clients-design.md) | ADR-002, 012, 014 |

All decision IDs link from the [ADR index](../adr/README.md). Use the [task worksheet](../development/implementation-workflow.md#prepare-a-task-packet) to record concrete paths, prerequisite evidence and the smallest complete behavioral increment before coding. Referencing a later integration test does not make the whole later segment a new prerequisite.

## Shared implementation checkpoints

1. Prove the retained upstream/native boundary with immutable inputs and observed effects before advertising support.
2. Establish state, idempotency, full artifacts and atomic reservations before admitting model/tool work.
3. Bind context and prepared work to current revisions, preserve user edits and reconcile uncertain effects.
4. Keep canonical acceptance separate from index visibility, and current access/deletion checks separate from historical snapshots.
5. Exercise pause and resume in a live CLI, with inspectors available and no new root or child work while paused; test owner loss separately.
6. Qualify all first-release capabilities together using current packaged evidence and owner review.

Task counts, owners and exact dependencies remain unchanged in this revision: 68 total, 56 first-release and 12 deferred. The new design documents and ADR records supply instructions; they do not change a task from planned to implemented.
