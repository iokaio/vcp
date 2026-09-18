# VCP plan work assessment

Assessed September 18, 2026 against plan revision 7 and the
[task ledger](plan/20-traceability.md).

This is a historical assessment from before P0 completion. All nine P0 tasks
have since completed bounded feasibility qualification; see the
[P0 handoff](development/p0-handoff.md). The estimates below have not been revised.

This page sizes the work laid out in each plan level. The [plan](plan/README.md)
itself contains no time estimates and its roadmap promises no dates. The
engineer-week figures here are a rough, unreviewed sizing based on each level's
task count, scope and acceptance criteria. They are not a schedule, a commitment
or acceptance evidence. They assume one experienced engineer and include the
tests and evidence each task requires.

The ledger remains the authority for task ownership, dependencies and state.
Recheck it before relying on the counts or states below.

## Estimate by plan level

| Level | Scope | Tasks | Estimate |
|---|---|---|---|
| **P0** | Upstream feasibility: pinned upstream sources, Codex baseline, lifecycle seam, storage/encryption comparison, Windows execution spike, Gemini fixtures, qualification ([01](plan/01-upstream-feasibility.md)) | 9 (3 complete, P0-03 in progress) | 10–14 wk total, about 6–9 wk remaining |
| **P1** | Engine core: domain state, internal commands/events, full capture, canonical backends with parity, budget ledger, projections ([02](plan/02-engine-state-and-capture.md), [03](plan/03-storage-and-budget.md)) | 6 | 9–12 wk |
| **P2** | Working coding loop: context, OpenRouter gateway, autonomy/grants, Windows tools, session loop, verification, recovery, compaction ([04](plan/04-context-and-instructions.md), [05](plan/05-openrouter-and-session-loop.md), [06](plan/06-windows-tools-and-recovery.md)) | 8 | 13–18 wk |
| **P3** | CLI: structured and interactive modes, inspectors, workspace continuation, history/pruning and portability commands ([07](plan/07-cli-and-inspection.md), [10](plan/10-history-and-pruning.md), [11](plan/11-encrypted-portability.md)) | 6 | 8–11 wk |
| **P4** | *Deferred* VS Code client: connection, views, versioned edits, inspectors, packaging ([18](plan/18-deferred-vscode.md)) | 5 | 6–9 wk |
| **P5** | Memory and portability: Munarium governance, ingestion, Tantivy, local embeddings/DiskANN, index generations, retrieval, pruning, encrypted snapshots, restore/handoff, integrated acceptance ([08](plan/08-memory-and-ingestion.md), [09](plan/09-local-search-and-generations.md), [10](plan/10-history-and-pruning.md), [11](plan/11-encrypted-portability.md), [15](plan/15-integration-and-release.md)) | 10 | 18–24 wk |
| **P6** | Routing: group registry, profiles with the Jev adapter, escalation/handoff, `/optimize`, profile qualification with held-out evaluations ([12](plan/12-routing-and-optimization.md)) | 5 | 10–14 wk |
| **P7** | Skills, MCP and visible delegation: discovery, bundled skills, MCP broker, task graph/worktrees, integration review, progress/recovery ([13](plan/13-skills-and-mcp.md), [14](plan/14-visible-delegation.md)) | 6 | 10–14 wk |
| **P8** | Release qualification: native Windows matrix, recovery/portability campaign, history/encryption review, distribution, upstream maintenance rehearsal, owner sign-off ([15](plan/15-integration-and-release.md)) | 6 | 7–10 wk |
| **P9** | *Deferred* public protocol, local server/attach, TypeScript SDK ([17](plan/17-deferred-api-and-sdk.md)) | 3 | 5–7 wk |
| **P10** | *Deferred* hooks, configuration imports, optional observers, other execution environments ([19](plan/19-deferred-extensions-and-platforms.md)) | 4 | 8–12 wk or more (P10-04 is open-ended per host) |

## Totals

- **First release (P0–P3, P5–P8):** 56 tasks, about 85–117 engineer-weeks. That
  is roughly 1.75–2.25 years for one engineer, and less with parallel work.
- **Deferred (P4, P9, P10):** 12 tasks, about 19–28 engineer-weeks.

## Observations

- **Largest level:** P5 combines three hard subsystems: governed memory, two
  index engines with coherent generations, and encrypted snapshot/restore. Each
  has crash, tamper and recovery acceptance tests.
- **Most schedule risk:** P2, from native Windows process control and
  reconciling unknown effects.
- **Parallel work:** the [execution waves](plan/README.md#execution-order) let
  some P5 and P7 tasks start alongside P2, and most of P6 can overlap P3 once the
  P2 loop and P5-06 retrieval land. Calendar time can therefore be shorter than
  the sum of the levels.
- **State at assessment:** P0-01, P0-02 and P0-07 are complete for their bounded
  qualification contracts and P0-03 is in progress. Every task from P1 onward is
  planned.

## Limits

- Estimates were produced by reading the plan documents and ledger, not from
  measured delivery velocity. No task in P1 or later has been started, so there
  is no project history to calibrate against.
- P0 qualification (P0-06) may replace a failed candidate with a bounded
  alternative, which would change the downstream figures.
- Live evaluation spend, hosted Windows qualification time and maintainer review
  latency are not included.
