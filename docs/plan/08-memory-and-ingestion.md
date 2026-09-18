# 08 — Munarium-derived governance and automatic memory ingestion

Status: planned. Owns P5-01 and P5-02. Requires canonical storage and selected Munarium components from P0. Read architecture section 11 and [search integration](09-local-search-and-generations.md).

## Implementation references and entry evidence

Use [governed memory requirements](../architecture/vcp-what.md#11-governed-memory)
and the [memory/retrieval design](../architecture/memory-retrieval-design.md)
for record shapes, gate ordering and replay behavior. Read
[ADR-008](../adr/008-local-governed-memory.md),
[capture](02-engine-state-and-capture.md) and
[canonical transactions](03-storage-and-budget.md) before adapting upstream code.
The design is proposed; source APIs and exact SQL/file schemas follow the P0 map.

The [JEV adoption map](../architecture/decision-evaluation-design.md#adoption-map)
allows later reuse of typed advice only within existing optional OpenRouter memory
extraction. Proposal/contradiction scores cannot accept truth, discard observed
history or replace evidence and governance gates. Keep local context/retrieval and
embeddings independent of that path; implement P5-01/02 without a P6 dependency.

P5-01 begins with P1-04's transaction/artifact contract and P0-02/P0-07's selected
Munarium revision, license/fixture inventory and demonstrated local dependency
closure. P5-02 additionally requires P1-03's retained event/artifact records.
Record these evidence identities in the work-item notes. A README describing an
upstream gate is not proof that the selected adapter compiles or conforms.

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

### Type registry, gates and durable resolution

Implement each claim class as a registry entry with its structured value schema,
normalization, subject/condition comparison, evidence requirements and default
acceptance rule. Commands use argument/cwd records and observed configuration;
module relationships use versioned source endpoints; architectural inferences
include inference status and conditions; fixes refer to patch and verification
versions. A remembered command remains evidence-bearing data and never grants
execution authority.

Port or wrap upstream gates in the documented order from the
[governance transaction](../architecture/memory-retrieval-design.md#governance-transaction).
Keep gate functions deterministic over supplied records so the same conformance
fixtures can run in-memory and against durable adapters. Map upstream outcomes
explicitly into accepted/disputed/rejected/configured-review plus a separate
evidence-status field. Preserve upstream expected outcomes and record every VCP
divergence, especially automatic acceptance and workspace scope.

Use one transaction for proposal, findings, immutable claim version, conflict or
supersession edges, head projection, event, index intent and idempotent response.
Check expected predecessor and relevant policy/scope revisions inside that
transaction. A lost acknowledgement returns the original receipt on retry. A
revision conflict reruns local validation against current records with a bounded
retry; it does not silently overwrite a concurrent correction. A different
payload under an existing proposal identity is an error, even if a model labels
both outputs equivalent.

Keep accepted observations from different source revisions distinct. Later test
failure changes current applicability; it does not rewrite the fact that the test
passed against an earlier revision. For historical inspection, resolve the
requested version first and then apply current access, deletion and retained
evidence checks. Return structured missing/redacted/purged evidence labels rather
than manufacturing a passage from its former summary.

Deliver the registry, thin upstream adapter, canonical repository adapter,
current/history projections and inspector finding types together. P5-01's commit
receipt must expose indexing as pending until segment 09 publishes a valid view.

## P5-02 — Activity and evidence ingestion

1. Consume durable user/task/model/tool/verification/child/correction events using a stored cursor and idempotent ingestion identity. Keep raw history separate from derived claims; a failure to extract a claim cannot delete the underlying activity.
2. Link code snapshots/diffs, file paths/symbols and check results to exact revisions. On reopening a workspace, compare observed state with the last known state and mark unobserved external actors unknown.
3. Implement deterministic extractors for explicit facts such as configured commands and recorded test outcomes. Route optional model-assisted lessons/architecture extraction through the same OpenRouter gateway, root or explicit maintenance budget and capture policy.
4. Queue bounded extraction/chunking/index work. Persist progress and retry causes; cancellation or a poisoned record cannot cause an unbounded hidden loop. Show accepted/rejected/disputed and pending counts.
5. Preserve workspace/folder boundaries, including multiple registered roots and nested repositories. No automatic transfer to unrelated workspaces or global user memory based on matching names.

### Cursor, work queue and extraction implementation

Implement the [ingestion algorithm](../architecture/memory-retrieval-design.md#ingestion-and-model-assisted-extraction)
with a durable cursor keyed by consumer/extractor specification and declared event
stream. Create pending jobs and advance the cursor atomically; jobs are the proof
that an advanced cursor has not discarded unprocessed observations. If one event
produces multiple claims, persist a stable output key for each. Store per-job
attempt, lease, failure and completion metadata with idempotent result references.
Lease expiration may cause another worker to retry, so correctness must not
depend on a lease providing exactly-once execution.

Add deterministic adapters in this order: verification results and configured
commands; explicit user preferences/corrections; source/diff/module observations;
child results and completed/interrupted work summaries. Each adapter checks the
origin artifact's capture availability and scope before reading content. Missing
content leaves a visible ingestion finding without losing the originating event.
Workspace reobservation records only observed differences and never attributes
unobserved edits to a human or agent merely from file timestamps.

For model-assisted extraction, create a normal accounted model attempt before
dispatch, retain the returned structured candidate artifact and submit validated
proposals through P5-01. Validate output size/depth, supported types and that all
claimed evidence IDs belong to the supplied authorized set. Prompt injection in
source text cannot choose policy, add global scope or authorize another call.
Malformed output is a visible failed extraction; any repair call needs its own
budget admission and bounded attempt limit. Ambiguous remote outcomes preserve
their charge liability through the ordinary gateway recovery path.

Use bounded queues with explicit per-workspace quotas, concurrency, batch bytes,
retry count and cancellation points. Record the selected limits and measure them
on the declared fixture; do not choose arbitrary values as release guarantees.
Yield local maintenance to interactive work and obey root pause/CLI close. Explicit
pause leaves inspection available with the CLI open, while task-scoped extraction
and model/child scheduling stay paused. Read-only status cannot resume that work.
Expose pending jobs, oldest lag, accepted/disputed/rejected totals and missing-resource
failures through existing inspection projections, without a second background
scheduler or hidden OpenRouter client.

## Tests and fixture design

Create `src/tests/fixtures/memory/` with two workspaces containing similar symbols but different conventions; verified commands and later failures; explicit and inferred architecture claims; contradictory source versions; and a complete activity stream with repeated delivery.

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

Until P0 provides the runner, these names identify required suites rather than
executable commands. Add contract cases under `src/tests/contracts/memory/` and
process-kill cases under `src/tests/recovery/memory_ingestion/`, registered in the
selected workspace. For each kill barrier compare raw event IDs, ingestion cursor,
pending jobs, claim versions and index intents after a fresh reopen. Independently
assert that each expected observation is accounted for; checking only claim counts
can hide one lost claim and one duplicate. Record selected upstream fixture IDs,
intentional differences, backend, policy/type-registry versions and actual attempt
costs without private source text.

## Exit

P5-01 is complete when automatic governed records survive retries/reopen and retain scope/evidence/history correctly. P5-02 is complete when all observed code-work classes feed durable ingestion without silent loss, duplicate promotion or unaccounted helper calls. Search readiness is completed in segment 09; pruning never bypasses the governance/delete rules established here.
