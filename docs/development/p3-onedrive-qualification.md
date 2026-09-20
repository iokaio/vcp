# Actual two-Windows OneDrive qualification

The production executable completed authenticated A → B → A restores through
actual OneDrive on 2026-09-20, in both backend directions. The two Windows
installations have distinct campaign-salted machine identities. The operator
transferred the executable, public enrollment metadata and independently exported
recovery keys separately; only encrypted snapshot objects traveled through
OneDrive. No model requests were made.

## Build and environment

- Production build: `32cf4b2a73db01e8f229411d614daff8c74dfbaa`, without qualification features.
- Executable SHA-256: `04f2fb86334195a6a2ea6c60a8f7c680e20f78387ead5c8741589a6e7608ceae`.
- Both machines: Windows `10.0.26200.0`, PowerShell `7.6.6`, OneDrive `26.163.0823.0004`.
- Machine A's observed vault filesystem: NTFS.
- Envelope: `vcp-signed-age/1`, age `0.11.2`, ed25519-dalek `2.2.0`.

## Transfer and restore evidence

| Direction | A outbound bytes | B return bytes | Observed sequence |
| --- | ---: | ---: | --- |
| Files → SQLite → Files | 641984 | 771824 | 2 → 3, deletion epoch 0 |
| SQLite → Files → SQLite | 641818 | 771740 | 2 → 3, deletion epoch 0 |

B observed the outbound objects at 12:39:12 and 12:39:53 UTC. A observed the
returned objects at 13:05:21 and 13:05:23 UTC. Both directions matched the
independently supplied ciphertext hashes and lengths. The returned objects were
initially absent on A while B's client was syncing, then arrived without manual
ciphertext copying. The overall account sync backlog was still in progress;
qualification concerns these specific objects, not all account contents.

Both actual return restores authenticated the descendant against A's preserved
sequence-2 trust checkpoint. Existing-root activation required the expected
descriptor hash and selected a separate canonical root and new workspace.
The checks verified three original source files and B's new marker, stable
workspace/session/root-task identities, settled/unresolved accounting of 50/67,
retained child lineage and governed claims. Restored work remained paused and
untrusted, without restored execution grants. Exact restore retry reconciled
without repeating rebind. The previous canonical and workspace file inventories
remained unchanged; the ephemeral store ownership token is excluded from this
comparison.

The native `restore_predecessor` qualification then passed both tests against
these actual roots (`p306-u04-return-native.log`, 2 passed in 3.54 seconds).
Each backend preserved the snapshot's original cut at watermark 58: 37 event
envelopes, 20 command receipts and 58 transaction receipts matched exactly.
The test also compared 51 retained canonical records and validated all 24
original artifact payloads (7,858 bytes) through both stores. Both predecessor
roots reopened successfully and retained their recorded file inventories.
Authority, binding, task presentation and search heads intentionally change on
restore; this comparison does not incorrectly require those mutable records to
retain old machine authority.

The focused CLI predecessor/history test Clippy check passed with warnings
treated as errors (`p306-u04-final-clippy.log`). All eight final repository,
harness and source-provenance checks passed
(`p306-final-u04-fast/5e464b18-2a95-40d1-8b7b-0e9894936c64/manifest.json`).

Search rebuilding truthfully reported an empty eligible lexical generation and
one excluded stale source record requiring trusted recapture. This fixture does
not prove nonempty search coverage or verification history. Separate native
tests qualify lexical recall, compatible retained MiniLM vectors, typed historic
receipts and accounting, as recorded in the [local qualification](p3-portability-qualification.md).

Raw receipts, operation journals and snapshots remain in local ignored artifacts.
The actual return evidence is under
`p306-u04-ready/return-A-811e9187-31d6-44e2-be44-375a83404ee0/`;
the independently supplied B receipt and its validation are under
`p306-u04-ready/`. These contain environment details and are not release inputs.

## Scope

This is real provider transport and authenticated restore evidence, additional
to the local negative/crash/conversion matrix. It does not claim that every
failure was injected on both physical machines, prove hardware power-loss
durability, discover a globally newest snapshot, or qualify the separate
P5-08/P8 integrated acceptance and distribution work items.

Together with the linked native matrix, this closes the required U04 handoff
gate for P3-06 and its P5-09/P5-10 prerequisites.
