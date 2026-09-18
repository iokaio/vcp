# Portable storage and authenticated snapshot qualification

P0-04's `src/crates/vcp-storage-spike` is a disposable comparison in the existing
Cargo workspace, not a supported persistence format or backup command.
[The storage design](../architecture/storage-portability-design.md) and
[ADR-003](../adr/003-canonical-storage.md),
[ADR-015](../adr/015-portability-and-storage-choice.md),
[ADR-019](../adr/019-cloud-encryption-and-keys.md) own the product contracts.

## Implementation map

| Source under `src/crates/vcp-storage-spike/src/` | Responsibility |
|---|---|
| `lib.rs` | Neutral events, claims/disputes, deletion epoch, unsettled reservation, artifact references and retained vectors |
| `store.rs` | Exclusive writer, optimistic sequence, command identity, SQLite WAL/FULL and checksum-chained framed transactions |
| `vault.rs` | Signed inner inventory, finalized age ciphertext, writer/lineage verification and artifact closure |
| `search.rs` | Actual Tantivy/DiskANN rebuild and reopen using restored text/vectors |
| `main.rs` | Disposable crash, benchmark, handoff and independent recovery commands |

Both backends persist identical complete neutral views. Content-addressed
artifacts are synchronized before commit. Each root has an exclusive OS file
lock and persistent backend identity. SQLite uses existing locked SQLx 0.9.0
and bundled SQLite with WAL/FULL. The file candidate uses bounded
length/inverse-length frames and a domain-separated SHA-256 chain. Only an
incomplete final append is discarded; complete corruption fails closed. A
persistence error seals the writer until reopen. Process-kill evidence does not
prove power-loss durability. Whole-view commits intentionally simplify this
comparison; production collections, migrations and compaction remain P1 work.

## Encryption and writer trust

Existing pinned Rust `age` 0.11.2 and `ed25519-dalek` 2.2.0 are the runtime
candidates. The age crate describes pre-1.0 releases as testing releases; this
is a qualified prototype choice, not an audited production cryptography claim.
P5/P8 must review supported versions and security notices. No new cipher, nonce
scheme or signature primitive is introduced.

The writer signs a domain prefix plus exact serialized manifest bytes. The
encrypted envelope carries its public identity, signature, canonical view,
parent ciphertext identity and complete artifact inventory. Restore requires a
separately supplied local writer trust set. An archive cannot enroll itself;
knowing the public recipient does not confer writer authority.

Full packaging embeds all artifacts in the encrypted envelope. Incremental
packaging encrypts artifacts separately and signs their plaintext/ciphertext
identities in a new encrypted manifest. Reuse is restricted to the same recipient;
recipient rotation re-encrypts artifacts. Writer rotation requires explicit local
trust replacement. Secret identities and signing keys never enter the vault.

Only a privately constructed finalized ciphertext value can publish. Resolved
staging and vault directories must be disjoint. Final encryption occurs outside
the vault; a ciphertext-only partial copy is renamed after synchronization.
Failed object publication leaves no accepted manifest. Content-addressed names,
size/count limits and exact artifact closure avoid generic archive extraction.
This prototype assumes trusted, stable local roots; hostile concurrent reparse
replacement and crash-tested activation remain P5/P8 work.

Restore verifies encryption completion, writer signature, minimum sequence and
deletion epoch, expected parent and every object. A fresh offline machine with
no trusted checkpoint cannot prove global freshness. Rotation cannot retract
old ciphertext or identities already copied elsewhere. Human-facing enrollment,
revocation, protected secret storage and recovery UX remain P3-06/P5-09.

## Reproduce locally

Use native Rust 1.98.0, Visual C++ tools, Node 24 and PowerShell 7. Acquire the
official Go age archive into an ignored directory, verify its hash, then extract.
Immutable release/archive/executable hashes and license are recorded in
`src/third_party/components/age-qualification.json`; it is a test tool, not a
runtime dependency or redistributed binary.

```powershell
pwsh -NoProfile -File scripts/test-storage.ps1 -AgeBinary artifacts/age/tool/age/age.exe -TargetRoot artifacts/upstream/codex-target -HandoffFixture src/tests/fixtures/portability
```

The wrapper verifies source inventories and compiles with `--locked`.
`scripts/upstream/qualify-storage.cjs` grades eight supervised crash cases, Go/Rust
age interchange and an independent Node Ed25519 verification. Four native
contract groups cover parity, corruption, recovery, signing and key rotation.
`src/tests/support/windows/storage-metrics.ps1` records process CPU, memory,
logical disk and vault marker observations. Sampled peaks are lower bounds with
the maximum sample gap recorded; debug fixture timings are not product promises.

Two scales and three repetitions compare full/incremental encrypted packaging
for both stores. Sequence one remains pinned while the writer commits sequence
two. Restore retains historical disputes, deletion metadata and liabilities.
The real DiskANN reader rejects incompatible cache bytes before a new generation
is rebuilt with Tantivy using retained inputs. Two synthetic 3D unit vectors
isolate portability from P0-02's separately qualified real embedding model.

The public ciphertext fixture in `src/tests/fixtures/portability` was produced
on the first Windows environment. A second environment receives the published
age unit-test identity separately from the vault, restores into both backends,
reopens and searches. That deliberately compromised identity must never protect
user data. Random identities in other tests remain local and are not uploaded.
The optional `storage_only` manual CI input uses standard `windows-2025` for
this bounded check. Ordinary PR checks remain fast Ubuntu checks.

P0-06 consolidates the evidence; P1 owns canonical transactions/accounting,
P5 owns scalable packaging, key lifecycle and activation, and P8 qualifies
packaged binaries and supported hosts. The bounded in-memory JSON encoding and
whole-view commit design require replacement at those production gates.
