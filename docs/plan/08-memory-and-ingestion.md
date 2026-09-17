# 08 — Munarium-derived governance and automatic memory ingestion

Status: planned. Owns P5-01 and P5-02. Requires canonical storage and selected Munarium components from P0. Read architecture section 11 and [search integration](09-local-search-and-generations.md).

## Code organization and contracts

Under `vcp-memory`, organize `claim`, `evidence`, `scope`, `proposal`, `gates`, `resolution`, `history`, `ingest`, `extraction` and `outbox`. Wrap selected Munarium kernel/gates behind VCP types; keep its database/server adapters out of the local runtime. Reuse upstream conformance fixtures with provenance and deliberate divergence records.

`ClaimProposal` includes stable proposal/origin IDs, type, subject/value, applicable workspace/root/path, source version, evidence IDs and idempotency identity. `ClaimVersion` adds memory sequence, predecessor/supersession links, recorded/valid time, resolution findings, acceptance policy and evidence status. `MemoryCommit` returns canonical receipt plus indexing intent; it cannot imply that both search indexes already include the claim.

Evidence links refer to retained source/output/check versions, not mutable paths alone. Store principal/workspace scope separately from branch/path applicability. Source observations, inferred architecture and user corrections must remain distinguishable after compaction and machine handoff.

## P5-01 — Governed writes

1. Implement schema/scope/evidence/predecessor checks for test/build commands, module relationships, architecture decisions, environment constraints, verified fixes and user preferences. Version the supported type registry.
2. Apply automatic acceptance policy to every supported class. Invalid proposals can be rejected, conflicting claims disputed, and inferred statements retained with unverified status; no routine approval queue is required by default.
3. Preserve contradictions and immutable versions. A correction creates a superseding version and records the reason/actor; a newer timestamp or confidence score cannot silently resolve contradictory evidence.
4. Commit proposal/resolution/version/event/indexing intent together. Identical proposal retries return the original receipt; different payload under the same idempotency key fails.
5. Implement current and historical claim projections. Historical reads still enforce current access/deletion policy and report missing retained evidence. A historical canonical view does not promise an available historical vector index.

Tests: duplicate proposal after a lost acknowledgement, missing or wrong-scope evidence, conflicting architecture inference, test success invalidated by later code, correction with bad predecessor and historical query after revocation. Compare files/SQLite outcomes through M01/E13. Inspect the actual records/indexing intents, not only returned text.

## P5-02 — Activity and evidence ingestion

1. Consume durable user/task/model/tool/verification/child/correction events using a stored cursor and idempotent ingestion identity. Keep raw history separate from derived claims; a failure to extract a claim cannot delete the underlying activity.
2. Link code snapshots/diffs, file paths/symbols and check results to exact revisions. On reopening a workspace, compare observed state with the last known state and mark unobserved external actors unknown.
3. Implement deterministic extractors for explicit facts such as configured commands and recorded test outcomes. Route optional model-assisted lessons/architecture extraction through the same OpenRouter gateway, root or explicit maintenance budget and capture policy.
4. Queue bounded extraction/chunking/index work. Persist progress and retry causes; cancellation or a poisoned record cannot cause an unbounded hidden loop. Show accepted/rejected/disputed and pending counts.
5. Preserve workspace/folder boundaries, including multiple registered roots and nested repositories. No automatic transfer to unrelated workspaces or global user memory based on matching names.

## Tests and fixture design

Create `tests/fixtures/memory/` with two workspaces containing similar symbols but different conventions; verified commands and later failures; explicit and inferred architecture claims; contradictory source versions; and a complete activity stream with repeated delivery.

| Test | Observable expectation |
|---|---|
| Replay ingestion after process kill | Each origin event creates at most its intended claim/version; raw history remains complete |
| Automatic architecture inference | Claim retained with evidence status, conditions and source refs; not presented as verified fact |
| Scope collision | Workspace B cannot retrieve or use workspace A's similarly named claim |
| Broken evidence artifact | Claim/extraction reports unavailable evidence; no fabricated source citation |
| Extraction budget exhausted | Raw activity remains retained, queued/model work stops and status explains backlog |
| External edit while VCP closed | Changed source recorded on reopen without attributing an unobserved action to the user or an agent |
| Backend conversion | Claim IDs, supersession/dispute history and source links preserved |

Run `memory`, relevant `store` and E13/E14/M01/M05/M07. Network-isolated tests use deterministic extraction; a separate bounded model-assisted fixture accounts for its actual attempts and uncertainty.

## Exit

P5-01 is complete when automatic governed records survive retries/reopen and retain scope/evidence/history correctly. P5-02 is complete when all observed code-work classes feed durable ingestion without silent loss, duplicate promotion or unaccounted helper calls. Search readiness is completed in segment 09; pruning never bypasses the governance/delete rules established here.
