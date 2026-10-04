# EE-02c: durable history separation and layout 3

Status: selected implementation contract under the [execution architecture plan](execution-architecture-review.md), October 4, 2026. The streaming encoder, full event-validation adapter and ingestion metadata adapter are prerequisites; this document does not claim that layout 3, history eviction or large-history qualification is implemented. EE-02d bounded reopen remains separate and unqualified.

## Decision and invariant

Modify the existing canonical Store and its Files/SQLite backends so the live owner retains bounded current records and sequence frontiers while events, command receipts, transaction receipts and original commit bodies remain durably readable. Preserve every canonical validation rule, transaction identity, duplicate-command outcome, fence, redaction rule and evidence reference. Preserve mandatory semantic validation of every retained transition on cold open, including an invalid interior transition followed by a valid final state. Persisted checksums and a final head alone never authorize skipping that validation.

Storage layout 2 already identifies a replay-base retention boundary. Use a distinct layout 3 identifier for the new representation. Do not reinterpret layout 2 as a non-destructive checkpoint, introduce an alternate execution driver, or add an engine-selection switch. The ordinary Store path changes in place. Retain prior generations and evidence throughout qualification; no deletion or retention relaxation is required.

## Representations and commitments

| Representation | Contents and commitment | Lifetime and limits |
| --- | --- | --- |
| Current state | Current records, current watermark and per-session sequence frontiers. Its versioned projection digest explicitly excludes historical payloads. | Live owner and immutable current snapshots. Preserve current-record count, individual-record and current-state capacity bounds. |
| Canonical history | Original retained commit bytes; ordered event envelopes; command and transaction receipts; existing pre-base prefix commitments. No invented transaction bodies before a retention boundary. | Durable pages and original journals, available through fallible bounded reads. Total retained history is independent of the current-state byte limit. |
| Authenticated history index | Ordered lookup pages for event ordinal/ID, command key, transaction ID and commit watermark, plus exact predicate dependency indexes described below. Page contents and child references are hashed under an owner-known root. | Bounded reader caches and fixed root descriptors. An in-memory map of every historical digest is an interim growing-metadata representation, not fixed-hot-state acceptance. |
| Legacy complete `State` | Existing archival/reference DTO with its exact v1 serialized shape. | Explicit materialization for supported legacy operations and differential tests. Never fabricate empty history fields to represent a live owner. |

Canonical transaction and receipt digests do not change. The existing whole-State v1 digest remains exactly reproducible when an operation requests that commitment: stream commands, events, records, sequences, transactions and watermark in canonical key/row order using the reference recursive row encoder. A current projection digest must never be substituted for that whole-State digest. Layout 3 records which digest each field means, its encoding version and its watermark. Historical redaction transformations retain their existing commitments and source/generation relationships.

An authenticated page read verifies its type, version, key range, order, count, bounds and digest through parent references to the root held by the validated live owner. A checksum stored beside a mutable row is insufficient. Bind original commit bytes, event ordinals, receipt identity and index entries to that root; do not trust an unbound receipt copied from the same mutable database. Keep the owner and snapshot generation pin alive for all reads. This replaces the integrity role currently served by resident validated receipt maps when those maps are evicted.

## Validation and transaction preparation

Keep one semantic implementation. Factor current mutation preparation and invariant checks into a pure preparation pipeline; both the complete-State reference adapter and asynchronous Store transaction path invoke it. The pure pipeline produces candidate current records, newly appended events/receipts and explicit historical obligations. `State::prepare` supplies those obligations from its complete in-memory DTO. `Store::transact`, already asynchronous, resolves the same obligations through bounded durable reads and authenticated indexes before publishing the same logical result. This contains asynchronous I/O at the existing owner boundary rather than making every domain validator asynchronous.

Fallible event adapters consume owned rows or page references, propagate read failures and finalize only a complete stream. All current-record dependencies are evaluated against the candidate current state, including changes that invalidate an older event. The original complete-state validator remains frozen in differential tests while checks are extracted. It is a correctness reference, not a second production execution path.

The required predicate inventory is explicit; an index is not admissible until the corresponding dependency proof and differential cases are complete:

| Predicate | Historical facts | Dependencies that require renewed validation |
| --- | --- | --- |
| Event identity and order | Envelope version, event ID, watermark, absolute order and per-session sequence. | Appended/replaced history, replay base, sequence frontier and watermark. Uniqueness and monotonicity are checked across page boundaries. |
| Event scope and artifact references | Event workspace/session/task and artifact IDs. | Every referenced session, task and artifact record; current workspace ownership and task/artifact session relationships. A later record change can invalidate an old event. |
| Event redaction | Exact redaction metadata, null data, absent execution metadata and original-content commitment. | Historical redaction transformation and current workspace deletion epoch. Retention rewrite remains an explicit validated operation. |
| Accounting send intent | Event ID, scope and `AttemptSubmitted` kind for each current attempt's send intent. | Attempt identity/scope/send-intent field, referenced event and current artifact/accounting records. Financial identity and no-duplicate-send rules remain intact. |
| Ingestion cursor and jobs | Absolute ordinal, boundary watermark, every selected event in each cursor prefix and each job origin's scope/kind. | Cursor scope/extractor/cut, job set/results, current task ancestry/root and result/proposal provenance. Never replace ordinal semantics with a watermark-only cursor. |
| Publication and redacted results | Existence and exact identity of prior transaction receipts; publication provenance. | Record mutation, current transaction exclusion, publication version and redaction transformation. The new transaction cannot masquerade as a prior receipt. |
| Idempotency | Exact transaction digest and command workspace/key/digest/result. | Transaction/command identity and immutable prior outcome. A mismatched duplicate must reject without changing current state. |
| Rewrite and export | Paired old/new events and receipts, retained artifact references, source and target commitments. | Explicit generation boundary, current deletion/authorization state and complete closure of retained provenance. Paging cannot change identity, order or omit protected evidence. |

The index must be derived from fully validated canonical input and updated atomically with the transaction that changes it. Reusing a predicate requires an exact inventory of every input that can change its truth value and a proof that none changed. Changed or ambiguous dependencies use full validation. Qualify acceptance, rejection, state and receipts against the frozen reference after every generated transition, including record changes, duplicate commands, rewritten history, invalid interior transitions and corrupted/missing index pages. Do not activate the previously rejected event-prefix shortcut merely because these adapters exist.

## Append and cold-open work

A naive implementation that rereads all durable events on every append preserves many checks but introduces O(history) disk reads per append and O(n²) work over replay. It is not final EE-02c acceptance. The ingestion adapter already demonstrates a safe narrower allocation: retain origin metadata only for current jobs and cursor metadata only for current cursors, and do no ingestion-history work when neither exists. Full event validation still runs independently.

Apply the same measured discipline to the remaining predicates. During the transition, report whole-history read fallbacks explicitly. Do not claim bounded append work until predicate indexes are qualified. Collect per-operation page reads, bytes read, rows checked, changed dependencies, fallback counts, index writes and retained metadata/cache size. Compare fixed current-record corpora with growing retained history on both backends. CPU or I/O regressions that prevent useful execution are unresolved acceptance, even if serialization allocations improve.

Cold open continues to replay every retained transaction semantically in order. Rebuild or verify index roots and derived metadata as part of that replay, retaining only the current records, bounded pages and necessary validation state. Hydrating a final index/root does not itself prove earlier transitions valid. EE-02d can optimize this only through a separate equivalent-integrity design; this contract does not authorize suffix-only validation.

## Durable publication and migration

For Files, preserve original journal bodies and publish immutable bounded history/index pages under a generation root. For SQLite, preserve canonical commit payloads and update current rows, history/index rows and the root descriptor in the same database transaction. Both representations implement the same logical obligations and digest definitions. Pages must accommodate every currently valid bounded row/commit; do not accidentally narrow existing transaction or record limits through a smaller page setting.

Migration obtains the ordinary canonical ownership lock and pins the selected source generation. It fully validates the source, builds a private candidate generation, and compares current state, exact historical bytes, receipts, legacy digests and available prefix commitments before activation. Flush candidate data and its descriptor before atomic selection publication. A crash before activation leaves the original authoritative; a crash after activation must resolve the exact published candidate. Resume uses generation identity and commitments, never directory existence alone. Keep original roots while readers or snapshots pin them, and preserve them throughout this evidence campaign.

Test process interruption at candidate creation, page publication, descriptor flush, selection publication and subsequent append. Test missing/truncated/swapped pages, forged keys/ranges/counts, checksum-resealed payload changes, stale roots, wrong backend/generation, interrupted conversion and source replacement. On-demand historical prefix/snapshot reads must retain the integrity guarantees of the already-qualified durable commit cursor.

## Explicit API and archive migration

`CanonicalStore::state() -> &State` cannot remain the live-owner interface while history payloads are evicted. Replace its live uses with current-record access and explicit bounded history queries. Keep a deliberately named complete archival snapshot/materialization operation for consumers that truly need it, with honest capacity/error behavior. Do not hide complete history reconstruction inside ordinary status, task inspection, verification admission or append. Migrate lifecycle projection/recovery, audit/export, snapshot jobs and CLI consumers in bounded package slices, preserving owner/revision/selection fences and scoped artifact access.

The existing neutral-history/1 archive has a 4 MiB aggregate bound and owns complete State/byte maps. Introduce neutral-history/2 with bounded ordered chunks and a bounded authenticated descriptor tree rooted in the archive manifest. Import/export, encrypted staging and recovery must stream those chunks end to end; assembling one giant JSON value, byte vector or descriptor list would reintroduce the same limit. Preserve original row/receipt semantics and permit streaming reconstruction of the exact legacy whole-State commitment. Retain explicit reading of existing v1 archives; unsupported new versions fail closed in old readers. An export that cannot represent a retained history must fail explicitly, never silently omit it or fall back to a misleading complete snapshot.

The 64 MiB legacy aggregate-State bound is removed only from the new durable-history representation after current-state and per-object limits are separately enforced. Keep legacy DTO materialization limits explicit. A large layout-3 store cannot be implicitly converted into a legacy bounded format; report that incompatibility or use the new streaming archive contract.

## Implementation checkpoints and acceptance

1. Complete the full-history validator adapters and predicate inventory, preserving frozen-reference equivalence. Finish receipt-dependent interfaces and the shared pure preparation pipeline; measure remaining whole-state work.
2. Specify and implement typed page/root codecs and both backend readers/writers, including owner-known authentication, exact canonical identity and crash recovery. No payload eviction before on-demand equivalence is proven.
3. Qualify transactional predicate indexes and invalidation dependencies, retaining full fallback for unresolved cases. Measure append/replay work; do not silently accept an O(n²) disk regression.
4. Migrate live Store ownership and callers to current state plus durable history. Retain complete State only for explicit reference/archival use. Prove immutable snapshots, duplicate/rejected transaction behavior and cross-runtime owner handoff remain correct.
5. Complete layout migration and neutral-history/2 snapshot/export/import recovery, then exercise retained history above 64 MiB with fixed current records. Verify every historical event/receipt, old/new digest, artifact reference and source commitment across reopen, conversion, rewrite, paging and interrupted publication on both backends.

Acceptance requires bounded hot payload/cache size as history grows, preserved full cold-open semantic guarantees, measured read/validation amplification, no incomplete archive success, and actual larger-history execution data. Compact in-memory metadata, copy-on-write snapshots and feed adapters are useful intermediate results; none alone completes durable separation or bounded reopen.
