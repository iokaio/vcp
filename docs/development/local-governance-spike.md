# Local governance qualification adapter

Task: P0-02. This is a bounded, volatile prototype inside `vcp-memory-spike`,
using the committed Munarium core and in-memory backend at
`8da666067000ca1ee9c131bc67e70b978862faa3`. It is not the production canonical
store and does not complete P5 memory ingestion or durable recovery.

The [qualification report](../evaluations/p0-02-local-governance.md) records
actual checks, source identities and remaining acceptance boundaries.

The [memory design](../architecture/memory-retrieval-design.md) and
[ADR-008](../adr/008-local-governed-memory.md) require retained evidence,
inspectable disputes, historical versions and workspace isolation. The
[local corpus experiment](local-memory-spike.md) supplies the real retrieval
path; this adapter replaces its direct `current` flag filter with retained
governance APIs. The flag remains only an independently authored result oracle.

## Concrete calls and ordering

`src/crates/vcp-memory-spike/src/governance.rs` holds a private `ScopedLedger`.
Each workspace receives a separate `munarium_store_mem::MemStore` and version.
No caller can select another store's version or pass an upstream raw claim ID.
Stable fixture document IDs map to the backend's generated claim IDs.

For one synthetic proposal, the adapter:

1. Checks the handle's workspace, unique document ID and expected canonical head.
2. Loads `storage::load_snapshot` at that head. A correction must target a
   current claim for the same subject within the same scoped handle.
3. Calls `gates::run_gates` with a `Candidate` and `ProposedClaim`.
4. Appends a `NewClaim` with `StorageBackend::append_claim`, repeating the head
   check inside the backend. Blocking findings produce a disputed record.
5. Stores the proposal's text digest, synthetic evidence label, workspace,
   stable predecessor ID and findings in the record's evidence. It also calls
   `record_findings` so the retained findings API can retrieve gate outcomes.
6. Resolves accepted visibility with `slice_facts` and `FactQuery`. Retrieval
   applies that view before limiting lexical or ANN candidates to three.

Exclusive mutable access serializes this prototype's snapshot/gate/write
sequence. Claim and findings writes are separate volatile operations. A
production adapter must place proposal, findings, resolution, edges and indexing
intent in the canonical transaction required by the architecture; this prototype
does not establish crash safety or acknowledged-write durability.

## Rejected supersession safeguard

The retained `ledger::resolve_slice` computes its superseded-ID set from every
claim at the selected pin before filtering accepted/disputed status. Therefore
an adapter that attaches a replacement edge to a rejected correction can hide
the accepted predecessor.

The VCP experiment retains a blocked correction as disputed evidence but does
not attach an effective `supersedes_id`. Its proposed predecessor remains in
`evidence.proposed_supersedes_document_id`; the attempted change and gate
findings remain inspectable. An accepted correction retains its actual edge.
The imported resolver and source inventory are unchanged.

This preserves the architecture's distinction between recording a proposal and
accepting a replacement. Tests lock an anchor, propose a conflicting correction,
and independently read the accepted predecessor, disputed proposal and retained
finding. A separate plain conflict test checks the ledger gate.

## Historical and retrieval evidence

Corpus version 2 names `atlas-pause-v1` as the explicit predecessor of
`atlas-pause-v2`. Texts and expected relevance are unchanged. Replay checks the
view immediately before and after the correction, then reads the original claim
to verify that its evidence remains present. The second workspace has a separate
`pause_session` meaning and store.

Each fresh query process reconstructs its volatile ledger from the compiled
public fixture, then opens the independently persisted Tantivy/DiskANN artifacts.
The process reports workspace record counts, governed current IDs, historical
checks and findings counts. The Node observer compares these to the independent
fixture oracle in both query processes. Random backend claim IDs are not used
as cross-process durable identities.

The fixture's `Witnessed` provenance records the synthetic input supplied to
the probe. Governance acceptance does not certify those descriptions as true or
claim the described product functionality is implemented.

## Remaining integration

The [offline CPU gate](offline-embeddings.md) and [declared resource experiments](local-memory-resources.md)
add bounded OS and scaling evidence to this adapter.
P0-04 qualifies durable storage; P5 integrates controller authority, immutable
evidence artifacts, idempotency, access revisions, deletion/tombstones and
transactional index intents. This bounded adapter provides no model-assisted
extraction, hosted server, PostgreSQL dependency or second production controller.
