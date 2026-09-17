# Governed memory and local retrieval design

Status: proposed implementation design for P5-01 through P5-07. No memory
runtime, index adapter or test harness is implemented by this document. The
[architecture](vcp-what.md#11-governed-memory) owns behavior;
[ADR-008](../adr/008-local-governed-memory.md) owns unresolved qualification
choices. Implement through [memory ingestion](../plan/08-memory-and-ingestion.md),
[retrieval](../plan/09-local-search-and-generations.md) and
[retention](../plan/10-history-and-pruning.md). The records below describe logical
contracts, not a selected serialization, database schema or upstream API.

## Boundaries and dependency direction

The controller supplies workspace identity, current authority, capture policy and
budget ownership. `vcp-memory` turns retained observations into governed proposals
and schedules local indexing through the canonical store. Its workers cannot
invent grants, bypass the model gateway or establish another ledger. Retain
selected Munarium governance behind adapters after P0 identifies its actual
interfaces and conformance cases; do not introduce its server deployment or
database provider as a local runtime prerequisite.

Keep operational history, evidence payloads, accepted claim versions and derived
search records distinct. An observed model statement is history even if no claim
is accepted. A rejected proposal remains inspectable under retention policy.
Search failure cannot roll back an acknowledged canonical claim, and successful
indexing cannot make an unsupported claim verified. The
[store and portability design](storage-portability-design.md#canonical-record-boundary)
defines the common transaction and artifact boundary.

## Logical records and identity

Use opaque stable IDs assigned by the domain ID service. Define sequence domains
explicitly: canonical transaction order, workspace memory order and session event
order are different; store their relation rather than comparing unlike integers.
Persist source revision/hash with source identity; a path by itself is not a
version. Mutable head projections point to immutable versions.

| Record | Required fields and constraints |
|---|---|
| `ClaimProposal` | Proposal ID, origin event(s), extractor/version, type registry revision, subject/predicate/value, conditions, workspace/root/path applicability, evidence IDs, expected predecessor, idempotency identity and payload digest |
| `ClaimVersion` | Claim/version/proposal IDs, canonical and memory sequence, predecessor and supersession edges, recorded and valid time, resolution, evidence status, accepting policy/actor, immutable structured value |
| `EvidenceRef` | Artifact/version ID, source revision, allowed scope, observation/verification kind, capture visibility, byte/line location where meaningful, integrity state; missing or redacted content stays explicit |
| `ConflictSet` | Stable set ID, conflicting claim versions, overlapping subject/conditions, finding and resolution links; no implicit winner by timestamp |
| `IngestionJob` | Event range/origin, extractor specification, bounded attempt count, last failure, model attempt if any, pending/leased/completed/deferred/failed state and committed result receipt |
| `IndexIntent` | Unique source transaction and operation, affected record versions, insert/supersede/delete action, target sequence, required embedding/schema specification, completion/failure reference |
| `ChunkRecord` | Stable source/version identity, chunker specification, ordinal plus span, scoped text artifact reference, embedding specification/vector reference and source lineage |
| `GenerationManifest` | Generation ID, canonical/memory watermarks, eligible record inventory identity, lexical/vector component IDs, mappings, schema/tokenizer/embedding versions, deletion epoch and validation receipt |
| `Tombstone` | Exact affected identities, action, effective sequence/deletion epoch, retention-policy revision and cleanup receipt; no removed passage copied into tombstone metadata |

Proposal retry identity includes workspace, origin and extractor version, plus
the extractor's stable output key when one event produces several proposals.
Persist that mapping instead of deriving identity from model text. Reusing the
same identity with different content is a conflict; return the original receipt
only when the stored payload identity matches. A newly configured extractor
version does not silently rewrite old claims: it produces linked new proposals.

Claims with overlapping conditions may conflict; claims from different source
revisions or mutually exclusive conditions may both be applicable observations.
For example, `test command passed at source revision A` remains true after a
failure at B. A claim that the command currently works needs a new applicability
assessment. Do not use a global confidence threshold to erase this distinction.

## Governance transaction

Implement a pure validation stage and a short optimistic commit stage:

1. Normalize the proposal against the versioned claim-type registry. Resolve
   workspace/root identity from the caller's authorized scope, never from text
   inside evidence. Preserve the original proposal for permitted inspection.
2. Load a coherent canonical view of evidence metadata, predecessor/head
   revisions, overlapping conditions and current policy. Validate source integrity,
   capture availability and scope intersection. Reject nonexistent or wrong-scope
   evidence identities. A valid evidence record whose payload is explicitly
   unavailable cannot support a verified finding; retain that limitation and
   accept an unverified proposal only when its claim-class policy permits it.
3. Run the selected Munarium gates through VCP adapters. Produce typed findings
   and one of accepted, disputed, rejected or configured awaiting-review outcomes.
   Automatic acceptance applies to supported classes by default; evidence status
   and resolution are separate fields.
4. Submit one transaction with expected claim head, policy and scope revisions.
   Append proposal/findings, immutable accepted/disputed version where applicable,
   supersession/conflict edges, memory event and indexing intent; update heads
   and the idempotent result together. Rejections need a durable proposal/result
   but no searchable accepted-claim document.
5. On revision conflict, reload and rerun gates with a bounded retry policy. Do not
   replay a stale resolution blindly or request another paid extraction solely
   because the commit conflicted. Recheck authorization before every retry.
6. Return canonical receipt and pending/ready/failed indexing status separately.
   The inspector links findings to evidence and records later loss of evidence
   without changing the original historical observation.

Current projections follow supersession and dispute state. Historical projections
select versions by the requested canonical/memory view, then apply *current*
authorization and deletion rules. An old sequence never restores an erased
payload. Record omitted history explicitly when retention prevents reconstruction.

## Ingestion and model-assisted extraction

Advance a durable event cursor only after each event has either a completed
deterministic result or a durable work item representing pending extraction.
Persist work creation and cursor advancement in the same transaction. A worker
lease prevents routine duplicate processing; idempotency still handles expired
leases and process death. Cursor position alone must never imply that pending
model work completed.

Start with deterministic adapters for recorded checks, configured commands,
explicit preferences and correction events. Each adapter returns structured
proposals with evidence references, not authoritative free text. Map child events
through their originating workspace/task/agent IDs so identical filenames in
different worktrees do not merge claims. Compare reopened source fingerprints
against the last observed revision and record an unknown actor for unobserved
external changes.

Optional semantic extraction reads only currently permitted captured content.
Construct its context through the ordinary context builder, reserve cost from the
originating task or explicitly configured maintenance budget and retain the model
attempt/result. Treat returned JSON as untrusted: size/depth bounds, strict type
validation and evidence-ID membership checks precede governance. The model cannot
add workspace scope, choose acceptance policy or execute a recalled command.

Persist retry causes and next eligible work state. Unsupported type, invalid
schema, inaccessible evidence and missing local assets are visible outcomes, not
unlimited retries. A transient store error can retry bounded local work; an
ambiguous remote model attempt follows the ordinary liability reconciliation
contract before any separately admitted attempt. Closing the CLI pauses owned
work; a durable queue is not authorization for an unattended daemon. Explicit
pause also stops new task-scoped model/extraction/child scheduling while leaving
the CLI open for history, memory, cost and pending-work inspection. Those reads
must not restart extraction or implicitly resume the task.

## Chunking, lexical fields and vector specifications

Chunk source and claim content separately. A chunker specification records text
decoding/newline normalization, boundary policy, overlap, maximum input length and
span mapping. Preserve exact source offsets or an explicit normalization map so
citations can identify the retained original. Oversized generated files and
unsupported encodings follow declared ingestion policy with omission reasons.

Lexical documents carry exact workspace/root/record/version/path/symbol fields
beside analyzed text. Preserve full identifiers while adding qualified case and
separator tokens. Tune field boosts against held-out truth rather than assuming
library defaults fit code. Index document ordinals never leave the adapter as
durable identity.

An embedding specification includes model and artifact digests, preprocessing,
chunker version, dimensions, scalar representation, normalization, distance
convention and runtime compatibility. Key vector reuse by the complete
specification and chunk content identity, while enforcing workspace permission at
every lookup. Shared bytes cannot create shared authority. Reject invalid lengths,
non-finite values and metric/specification mismatches before a vector is persisted.

Use pinned local assets and bounded CPU batches. Query embedding uses the same
compatible preprocessing/model specification as its vector generation. Missing
assets, cancellation and memory pressure return explicit degraded/not-ready
results. No adapter may select a remote embedding endpoint. Retained vectors can
rebuild a compatible graph when binaries cannot reopen; a new model requires new
vectors and a new generation, with changed ranking provenance.

## Generation publication and recovery

Treat lexical and vector components as one advertised view. Suggested build
metadata progresses through `queued`, `building`, `validated`, `published`,
`retired` and `failed`; these are worker states, not product-task ledger states.

1. Pin canonical view N and enumerate eligible records plus intents up to N.
   Record authority/deletion revisions and explicit per-record exclusions. A
   watermark means every required intent through N is represented or has a
   declared terminal omission; it is not the highest successful record ID.
2. Build components in uniquely owned private local directories. Persist stable
   ID mappings and inventories. Validate the intended eligible record set across
   both components, with explicit differences for records lacking usable vectors.
   Such differences must report degraded coverage instead of claiming full hybrid
   visibility.
3. Finalize files, validate sizes/integrity, reopen readers and verify component
   compatibility. Recheck deletion state before publication; current tombstones
   still govern queries if a later deletion races the build.
4. Publish a canonical manifest and switch the authoritative active-generation
   reference in a revision-checked store transaction after the component files
   reach their qualified durability boundary. A filesystem convenience pointer,
   if used, is reconstructable from that transaction and cannot override it.
5. A lost reply is handled through the publication transaction's idempotency
   receipt. Reopen loads only referenced validated components. Unreferenced build
   directories are recoverable work or cleanup candidates; directory recency is
   never proof of publication.
6. Retire old components after reader, snapshot and supported historical-view
   references release. Serialize pin acquisition against retirement; a cleanup
   worker cannot delete files between selecting a manifest and pinning it.

Use a bounded overlay for records newer than the active generation only when its
coverage is explicit. Pin its canonical range and version with the query. Bound
record count, bytes, scan time and resource use; exceeding a bound returns lag or
not-ready status and schedules normal indexing. Do not label a lexical-only
overlay as fresh semantic recall. If `required_seq` cannot be satisfied within
the caller's wait limit, return unsatisfied freshness with available watermarks.

## Query authorization and context handoff

Resolve the caller's current workspace/root/path scope before obtaining candidates.
Keep applicability (branch/source revision) separate from access authorization.
Search adapters can use scoped partitions or filters, but the canonical service
is the final eligibility oracle. Candidate-only search APIs should avoid returning
stored passage text before this check; if a library yields text internally, keep
it inside the trusted adapter and out of logs, inspectors and model context until
authorized.

Pin the generation and overlay, embed locally, retrieve bounded lexical/vector
candidates, then batch-load current eligibility metadata. Suppress revoked,
excluded, purged or inapplicable records. Fetch retained evidence only for eligible
IDs. If authority/deletion revisions change during the read, revalidate before
return or fail with a bounded retry result. Counts and exclusion explanations
must not reveal names or content outside the caller's inspect authority.

Fuse *ranks*, not incomparable raw score scales; reciprocal rank fusion is the
initial experiment in [architecture 13.5](vcp-what.md#135-query-algorithm). Version
weights, rank constant, deduplication and tie-breaking. Deduplicate overlapping
source spans while preserving all relevant evidence IDs. Fit final passages to
the request's byte/token limits without truncating away uncertainty labels or
fabricating citations. Exact-subset vector fallback, when qualified, is bounded
and visibly degraded; it never replaces required DiskANN acceptance evidence.

The returned context material carries generation, canonical/memory watermark,
authority revision, deletion epoch, source/version IDs and evidence availability.
The controller revalidates that manifest at the model send boundary. Serialize
prune/revoke application and dispatch admission through the controller's fence:
either dispatch admission linearizes first with actual submission certainty
recorded separately, or deletion linearizes first and blocks the stale dispatch.
Admission alone does not prove bytes were sent; cancellation may still prevent
transfer. An epoch comparison performed long before admission is insufficient.
Bytes already sent cannot be recalled; record that limitation while invalidating
later reuse and cached context.

## Failure and acceptance oracles

| Boundary or failure | Required observation after retry/reopen | Owning evidence |
|---|---|---|
| Lost governance reply | One proposal result, one intended version, original receipt | P5-01 / M01, E13 |
| Cursor advances then worker dies | Durable pending job remains; no missing observation or duplicate promotion | P5-02 / E14, M07 |
| Claim conflicts with correction | Immutable history and dispute/supersession links; no silent newest-wins resolution | P5-01 / M01 |
| Local model missing or incompatible | Raw history/claims retained, explicit vector coverage deficit, no remote fallback | P5-04 / U09 |
| Kill during component or manifest write | Last published valid generation or explicit rebuild state; no mixed readers | P5-05 / M02 |
| Revoke/purge during old-generation query | Forbidden passage does not reach newly admitted context or diagnostic text | P5-06/07 / M05 |
| Narrow authorized corpus | Bounded candidate work and reported recall degradation; no scope widening | P5-04/06 / M04 |
| Tokenizer/model upgrade | New compatible generation, old pins preserved, changed rankings identified | P5-05 / M06 |
| Disk full or Windows file lock | Canonical receipts remain valid; retryable cleanup/build failure visible | P5-05/07 / M07 |

Build synthetic duplicate-symbol workspaces, versioned test outcomes, explicit and
inferred architecture claims, and secret-marker deletion races. For retrieval,
store held-out query/relevant-ID judgments independently of model-visible inputs;
measure lexical/vector/fused precision and recall separately and ANN neighbors
against exact distance over the same authorized vectors. Report cold/warm latency,
embedding/build costs, RAM/mapped pages/disk and actual corpus identity. P0/P8 set
numeric thresholds before scoring; no illustrative value here is a release claim.

See [test fixture contracts](../plan/16-test-fixtures-and-acceptance.md),
[storage cleanup and restore](storage-portability-design.md#retention-and-prune-transaction)
and [ADR-016](../adr/016-history-and-pause.md) for cross-feature acceptance.
