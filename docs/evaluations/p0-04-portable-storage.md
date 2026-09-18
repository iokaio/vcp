# P0-04 — Portable storage and authenticated encryption

Status: P0-04 bounded feasibility complete on September 18, 2026, following
local and second-Windows-environment qualification. These results do not complete
P1 canonical backends, P5 portability or the U04 release acceptance matrix.

The [implementation guide](../development/portable-storage-spike.md) maps source,
commands, trust assumptions and remaining production work.

## Reproduction and inputs

```powershell
pwsh -NoProfile -File scripts/test-storage.ps1 -AgeBinary artifacts/upstream/age-v1.3.2/age/age.exe -TargetRoot artifacts/upstream/codex-target -HandoffFixture src/tests/fixtures/portability
```

The local native runs use Windows 10.0.26200/NTFS, Rust 1.98.0
(`88d9e12ae`), MSVC 14.50.35717 and an unoptimized debug build.
No paid calls, private corpora or WSL are used. The independent tool is Go age
v1.3.2 at `b74dce4cdbe35b5e5f66c06d9612b72f89028758`; its archive and executable
hashes are checked against `src/third_party/components/age-qualification.json`.
Runtime candidates are locked age 0.11.2, Ed25519-dalek 2.2.0, SQLx 0.9.0,
Tantivy 0.22.1/index format 6 and DiskANN 0.56.0. The source patch adds only
the original comparison package to the existing workspace/lock graph.

Independent Codex reconstruction matches all 7,938 files, result digest
`a0600ed39acc8359279bd3e58ee2b6d9f4a3c2817572ac2de3ca3bb1543eb00a`.
Patch 0009 has SHA-256
`d106b4928be01b276c6268a568c34dcd9a6df401e7fa65b2ad92874d1d43707a`.
Original crate, runner and tool-pin hashes are recorded separately in each
native manifest and checked for changes during execution.

## Local acceptance

The initial complete run is
`artifacts/storage/f8c6686d-1332-48b9-9a83-680b075d95b5/manifest.json`.
The follow-up
`artifacts/storage/fec5f4bb-3cd2-4168-9211-e1f64b56995f/manifest.json`
also validates the committed public ciphertext fixture using a separately
supplied recovery copy. Both exited zero. A same-host fixture check does not
replace the pending second-host gate.

That second-host gate subsequently passed in
[the standard Windows run](https://github.com/iokaio/vcp/actions/runs/35364917717)
at implementation commit `45ac45749132fe0d8ac8572b4c983c5df55538cd`.
The downloaded manifest is
`artifacts/p0-storage-hosted/f641c492-3831-486b-b89a-76c70d9247f9/manifest.json`.
It records Windows 10.0.26100, MSVC 14.51.36231, native Rust 1.98.0 and binary
SHA-256 `d6e7e29f690ce151cb5bfd174d5cbcd46678be7812278ad78492ad049a67dd8c`.
All four contract groups, eight crash cases, 24 packaging rows, independent
cryptographic checks and first-machine fixture restore passed. The latter
opened both backends and rebuilt real search using a separately supplied public
test identity. Only this bounded manual job ran; the full native suite was skipped.

The conversion-measurement follow-up is
`artifacts/storage/5b592ec0-97b1-4a44-905f-fb2424be7a55/manifest.json` (exit zero).
It also measures backend conversion and queries SQLite's actual journal and
synchronization settings. Its binary SHA-256 is
`137878733d1c1221b1541544b07a3326766c2ee7112f3fd7db1e3ecd0dd9313e`.

Current-source acceptance is
`artifacts/storage/10c80c20-e35c-4faa-8add-0f0c941fbe26/manifest.json`, exit zero,
binary SHA-256 `6086c53896efd99e6ff6392101184c7653c277bc5c38dc6bf24d0af181f8ca28`.
It includes the additional negative identity/lineage/deletion regressions and
per-format CPU and staging-plus-vault disk measurements. For the first 104-record
SQLite repetition, full versus incremental first-encryption CPU was 500,000
versus 78,125 microseconds and sampled snapshot disk was 2,721,626 versus
231,653 bytes. Conversion took 55,721 microseconds SQLite-to-files and 97,898
microseconds files-to-SQLite in the corresponding first repetitions. The driver
observed SQLite 3.51.3, `journal_mode=wal`, `synchronous=2` (FULL).

| Contract | Observed result |
|---|---|
| Backend parity | Identical neutral export/reopen; conversion in both directions preserves IDs, disputes, deletion epoch, generation and unsettled reservation |
| Writer/revision | Exclusive owner lock; stale revision, command identity conflict and backend identity mismatch reject |
| Durability barriers | Eight real process kills: before write, partial/uncommitted write, committed-before-ack and after ack for both stores; reopened sequence matches the independent acknowledgment observer |
| Corruption | Missing artifact, complete frame corruption and invalid history reject; incomplete final tail repairs only to the previous complete frame |
| Snapshot consistency | Sequence one stays pinned during the next writer commit; new sequence two is present after reopen |
| Writer authentication | Stranger with recipient knowledge rejects; unsigned/invalid signature rejects; Node independently verifies Rust Ed25519 output |
| Encryption | Rust-to-Go and Go-to-Rust age interchange pass; wrong key, altered header/payload, truncation and missing object reject |
| Publication | Disjoint staging enforced; injected before-publication and incomplete-object upload failures produce no accepted manifest; only finalized ciphertext can publish |
| Rotation/replay | Recipient rotation re-encrypts; writer revocation rejects prior writer; local sequence/deletion/parent pins reject replay or divergence |
| Search recovery | Real DiskANN rejects incompatible cache bytes; restored text/vectors rebuild Tantivy/DiskANN and pass the independent known-result query |

Four native contract groups and 24 measured packaging rows pass. Synthetic data
contains 100 or 1,000 event records plus four governance/accounting records,
one unchanged 139,264-byte artifact, and two 3D unit vectors. Each backend and
packaging mode runs three repetitions. This tests portable input retention;
P0-02 separately qualifies real embedding inference and larger retrieval corpora.

Representative first-repetition encrypted results from the initial run:

| Backend / records | Full changed bytes | Incremental changed bytes | Full restore ms | Incremental restore ms |
|---|---:|---:|---:|---:|
| SQLite / 104 | 1,361,128 | 46,295 | 477 | 74 |
| Files / 104 | 1,361,123 | 46,327 | 467 | 72 |
| SQLite / 1,004 | 1,744,647 | 429,834 | 694 | 221 |
| Files / 1,004 | 1,744,681 | 429,857 | 705 | 228 |

The complete native benchmark took 40,358 ms wall and 38,312.5 ms process CPU.
Observed working-set peak was 34,836,480 bytes; sampled private-memory peak was
16,855,040 bytes and aggregate logical disk peak 48,044,937 bytes across retained
repetitions and temporary files. The 213 samples requested 50 ms spacing but had
a 399 ms maximum gap. These are sampled lower bounds, not exact disk/private
peaks. There were 5,302 ciphertext observations with no plaintext canary found.
That marker observation supplements independent format interoperability; it is
not the cryptographic proof. Search rebuild/query took approximately 136–180 ms
in these representative rows. Do not compare these debug fixture timings to
release application performance.

## Decisions and limitations

Select SQLite WAL/FULL as the default integration candidate for its existing
transaction machinery; the file prototype demonstrates feasible parity but
leaves greater replay, corruption, checkpoint and migration work. No prototype
format is advertised as a supported backend.

Prefer immutable encrypted artifact reuse for the next snapshot design. The
whole-view JSON envelope intentionally amplifies full-package size and still
rewrites canonical state in incremental mode. Production P5 must replace that
bounded representation and repeat measurements; these savings are fixture-specific.

Age encryption and Ed25519 writer signatures are separate checks. Trust is
enrolled locally, never from snapshot contents. A fresh offline machine cannot
prove newest global state without an independently trusted checkpoint. Rotation
cannot revoke copies already possessed by someone else. Age's pre-1.0 testing
status remains a production dependency-review obligation.

The public handoff fixture has no user data or private recovery material. It
uses age's published unit-test identity and a fixed synthetic writer seed.
Production operator enrollment, protected secret storage, automatic recovery UX,
hostile root replacement, power-loss behavior and atomic activation remain
P1/P3/P5/P8 work. Raw local artifacts and random test keys are not published.
