# 11 — Developer keys, encrypted backups and machine handoff

Status: planned. Owns P5-09, P5-10 and P3-06. Requires qualified P0 encrypted-storage/key/writer-trust decisions, stable store/generations/pruning and workspace continuation. Architecture sections 12.8–12.11 and ADR-019's confirmed boundaries are binding; its format/library/writer-authentication choices still require P0 qualification.

## Supporting design and entry gates

Read [portable state requirements](../architecture/vcp-what.md#128-portable-environment-and-cloud-folder-transport),
[encryption/key requirements](../architecture/vcp-what.md#1211-local-plaintext-encrypted-vaults-and-developer-keys),
[ADR-015](../adr/015-portability-and-storage-choice.md),
[ADR-019](../adr/019-cloud-encryption-and-keys.md) and the proposed
[storage/portability design](../architecture/storage-portability-design.md).
The latter specifies manifests, pinning, cleanup, path validation and activation
ordering; it does not choose a cryptographic construction.

P0-04/P0-06 must supply immutable format/library/dependency identities,
interoperability results, authorized-writer trust/enrollment design, supported
key lifecycle and native Windows failure evidence. P5-09 also requires P1-04 and
P5-05/P5-07; P5-10 requires P5-09/P3-04; P3-06 requires those services and P3-04.
Preparatory interfaces can be developed earlier, but no fake crypto, unsigned
writer substitution or unrun experiment satisfies these dependencies.

## Code organization

| Proposed module under `vcp-store` | Responsibility and interface |
|---|---|
| `snapshot/view` | Pin canonical snapshot plus artifacts/generations; return immutable watermark/reference set |
| `snapshot/manifest` | Versioned inner inventory, ancestry, scopes, deletion epoch, liabilities and compatibility |
| `keys/registry` and `keys/recovery` | Public recipients/key refs, local secret handles, independent recovery check and rotation |
| `snapshot/authentication` | Apply/verify the authorized-writer trust contract selected in P0; separate from encryption |
| `vault/encrypt` and `vault/publish` | Existing crypto library, finalized ciphertext receipt, opaque-name publication and status |
| `restore/stage`, `restore/validate`, `restore/activate` | Local decryption, safe extraction, integrity/lineage checks and atomic activation protocol |
| `vault/retention` | Retained snapshot/key references and explicit cleanup obligations |

The publisher accepts a typed finalized encrypted-object descriptor, never a plaintext artifact path or generic byte stream. Keep test-only fake crypto outside release builds. Active code/history/SQLite/files/index data and private local staging remain plaintext; every object entering the portable vault is ciphertext, including catalogs/manifests.

## P5-09 — Key lifecycle and publication

1. Implement key create/import with the qualified library and an independent recovery export path outside known/declared sync roots. Ordinary configuration contains public recipient/key reference, not the secret. Secret input/output bypasses model and transcript capture. Optional local credential caching cannot be the only recovery copy.
2. Verify an encrypt/decrypt round trip from independently saved recovery material before enabling automatic cloud backup. Pin the developer-selected recipient and writer-trust configuration; repository text, model output and restored configuration cannot replace it.
3. Capture one consistent canonical view, pin all referenced payloads and compatible index generations, and record pending indexing work. Include full activity/claims/child state, routing/optimizer policy, budget liabilities, workspace checkpoint and local rebuild inputs. Exclude live authority, secret keys, handles and machine-bound credentials.
4. Build the inner manifest with hashes/lengths/versions and ancestry/deletion metadata. Apply the approved writer-authentication contract inside the protected format. Encrypt the entire archive through the pinned age/Rust candidate or ADR-selected replacement, using its prescribed randomness and finalization.
5. Stage plaintext and ciphertext locally outside sync roots. After encryption finalizes and closes successfully, copy ciphertext under an opaque immutable vault name. A partial vault copy contains ciphertext only. No plaintext sidecar, path-based filename or recovery identity may appear.
6. Record `locally_published`, observable transfer status and `restore_verified` separately. Missing keys/configuration, crypto errors, disk exhaustion or unavailable vault leave local work intact and backup pending/failed. There is no plaintext fallback.

Automatic triggers are completion, controlled pause/exit and explicit backup after setup; a bounded interval is optional. Public-recipient encryption can work without loading the secret decryption identity, but any required writer-signing material follows the separately selected local credential lifecycle. Never bypass writer authentication to make unattended backup succeed.

### Key enrollment and publisher construction

Implement secret-bearing operations through a dedicated local interaction path
that bypasses command-history, ordinary events, model prompts and JSONL argument
capture. Persist public recipient/key reference, recovery verification outcome
and trusted writer configuration separately from secret handles. Validate recovery
using the independently saved copy; a successful round trip using only an
already-loaded process secret does not prove an export is usable. Wrong/missing
material produces actionable status without logging its contents.

Construct the publisher's accepted ciphertext type only in the production
encryption adapter after successful stream finalization/close. A plaintext path,
unclosed encoder or generic byte reader cannot implement the same public
publication entry point. Record encryption-format identity and the local operation
receipt without emitting protected inventory/hash metadata into the vault.
Compile-time boundaries supplement real error-path tests and do not themselves
prove cryptographic confidentiality or writer authenticity.

Use a durable snapshot job holding canonical and generation pins. Persist its
watermarks/reference closure before lengthy I/O so restart can account for every
owned staging directory and pinned object. Include full retained task/claim/source
history, unfinished child/effect state and liabilities; record exclusions and
missing dependencies. Keep credentials, new-host execution grants and recovery
material out of the portable record set even when protected full capture is on.

For publication, follow the
[staging/encryption protocol](../architecture/storage-portability-design.md#key-lifecycle-and-encrypted-publication).
Resolve Windows paths and reparse points, validate local/vault separation, prepare
the inner manifest and finalize ciphertext locally. Copy to a new opaque immutable
name and record local publication only after the owned copy completes. A retry
must reconcile an existing owned destination by receipt/identity rather than
overwrite a different object. Cloud delivery order is not controlled by VCP;
incomplete hydration/object sets remain unrestorable regardless of source order.

Recheck deletion policy before publication. A job pinned before a purge must
rebuild or remain pending when its payload would violate the current publication
policy; retained old snapshots and copies already admitted before that purge are
reported separately with their cleanup/retention state. Cleanup releases only
the job's pins and owned staging objects after their obligations are resolved.
Disk-full, locked-file and encryption failures remain visible jobs with bounded
retry, preserving the last valid snapshot and local task durability.

Explicit pause with the CLI still open may trigger an already configured local
snapshot. It must not resume task-scoped model extraction, child work or model
requests. Users can inspect backup/cost/history state while paused. Closing the
CLI allows only its bounded controlled-shutdown work; unfinished publication is
recorded for deliberate continuation rather than creating a hidden worker daemon.

## P5-10 — Restore and sequential handoff

1. Hydrate/download encrypted objects into local staging. Supply the developer's independent recovery identity; authenticate/decrypt the full stream including final-chunk detection before activation.
2. Verify writer trust, inner inventory, bounds, archive paths/link behavior, hashes, schema versions, source references, generation compatibility, deletion epoch and ancestry. Reject encrypted-but-untrusted new-writer content and valid older replay according to the chosen trust/lineage contract.
3. Extract into an isolated plaintext local root. Apply resource/expansion limits and ensure no archive entry escapes it. Wrong/missing keys or any failed validation preserve the active environment unchanged.
4. Rebind workspace roots, local credentials/tool dependencies and authority; restore liabilities and unfinished children. Open compatible indexes or rebuild locally from retained sources/vectors while retaining provenance and exposing progress.
5. Activate the validated root through a crash-tested pointer/switch protocol. Preserve the last valid root until the new one reopens successfully. Backend conversion uses the same neutral record contract and validation.
6. For A-to-B-to-A handoff, preserve stable workspace/task IDs and snapshot parent relationships. Offline descendants remain separate until explicit branch selection or supported reconciliation. No filename timestamp merge or cloud-wide writer-lock claim is permitted.

Rotation creates/verifies a new identity before selecting it for future snapshots. Old snapshots require their old identities unless locally re-encrypted into new verified bundles. Losing all recovery keys makes those cloud snapshots unrecoverable; surviving plaintext local data can start a new encrypted lineage. Rotation cannot retract already obtained keys/ciphertexts.

### Validation pipeline and activation journal

Implement restore as a persistent operation with ciphertext acquisition, local
decryption, writer validation, inventory verification, safe extraction, canonical
import, environment rebind and activation stages. Bound input bytes, expanded
bytes, entry count and nesting before allocating from untrusted manifest values.
Tentative plaintext must remain in private local staging until the entire
authenticated stream has passed final verification.

Reject duplicate archive names, unsupported mandatory fields, malformed sizes,
missing objects and invalid links before activation. Windows tests include
drive-relative/absolute/UNC paths, traversal, alternate streams, reserved names,
case/normalization collisions and junction-based redirection. Path containment
requires handle/reparse-aware checks from the qualified implementation, not only
string-prefix comparison. Never run restored setup commands to discover whether
the archive is valid.

Verify writer trust against independent local enrollment; the archive cannot
authorize its own writer or change the next-backup recipient. Compare ancestry and
deletion state with trusted current knowledge. A fresh machine cannot infer that
an otherwise valid snapshot is globally newest from the archive alone. P0's
writer/head enrollment procedure must define the trusted freshness input and
diagnostic limits; do not invent universal replay protection. Divergent branches
retain their identities and tombstone histories rather than taking the highest
integer epoch across unrelated lineages.

Use a neutral logical export comparison to validate backend conversion: stable
record IDs, version edges, source integrity, memory/canonical sequence relation,
task graph, pending intents and exact unsettled/settled ledger amounts. Rebuild
incompatible index binaries locally from retained source/vectors and record the
actual resulting generation. Successful canonical import and ready search are
separate status fields.

Activate under a destination writer/activation lock with an expected-current-root
check and durable intent referencing old root, staged root and validation receipt.
Publish through the tested pointer/switch protocol, reopen the selected root and
only then admit normal writes. Kill tests must show how each boundary resolves
without mixed roots. Preserve the prior valid root under retention policy; after
new writes, rollback needs reconciliation and cannot silently discard them.

Workspace rebind is separate from data-root activation. Compare recorded base,
dirty changes and required untracked files against the destination, then use the
prepared revision-checked edit path for any materialized workspace change. Keep
tasks paused until dependencies, local authority and uncertain effects/charges
are reconciled and the user deliberately resumes. Restoring context is not a
grant to execute on the new host.

## P3-06 — User commands and diagnostics

Implement `storage configure/migrate --preview`, `backup keys create/import/verify/rotate`, `backup configure/create/status`, `restore --workspace --key`, and `workspace rebind`. These commands orchestrate the services above; never place literal secret material in argv or ordinary JSONL.

Show unsynced sequence range, selected key reference, independent-recovery verification, missing tools/secrets and whether a snapshot was merely published or actually restored. Without decrypted local metadata, browse opaque vault objects only. Local exports may be plaintext outside sync roots; cloud-directed exports use the same publisher. `doctor` checks path/junction separation without claiming to detect every unrelated synchronization program.

### CLI projection and error contract

Route commands through typed controller operations with idempotency and expected
revisions. Keep long-running snapshot/restore progress durable and inspectable
after reconnect; cancellation reports whether it happened before publication or
after an immutable ciphertext object already exists. Read-only status remains
usable during explicit task pause and cannot implicitly retry paid work.

Distinguish `recovery_not_verified`, `writer_not_trusted`, `vault_unavailable`,
`ciphertext_incomplete`, `integrity_failed`, `lineage_conflict`,
`format_incompatible`, `rebind_required` and `search_rebuild_pending` as proposed
typed diagnostic categories. Error text includes permitted object/key references
and next local action, never key bytes or plaintext manifest data from an
unauthorized snapshot. Headless mode emits the same stages and limitations.
Ordinary configuration imports cannot suppress trust/recovery checks.

Migration/restore preview reports target root, expected active-root revision,
workspace collision risk, required/free staging space, index compatibility,
required tools/secrets and retained recovery roots. A successful preview is not
an activated environment. Preserve user decisions through resumable commands,
but reject stale activation authority if the destination changes while staging.

## U04 test campaign

Use disposable Windows environments A/B, separately provisioned keys and a controllable sync-folder copier. Run with both storage preferences and conversion in both directions.

| Test | Independent evidence |
|---|---|
| A-to-B-to-A round trip | Original and restored canonical IDs/history/claims/budget balances match; pending work and indexes usable |
| Vault observation | Monitor files as created, including partial/error paths; no plaintext source marker, manifest, hash inventory or secret identity appears |
| Cryptographic conformance | Independent format decrypt/verify succeeds for correct identity; wrong key/header change/payload change/truncation fail |
| Writer substitution/replay | Public-recipient knowledge cannot create an accepted writer; known stale snapshot cannot silently replace newer lineage |
| Root/archive attack | Traversal, absolute paths, junctions and excessive expansion fail before active-root modification |
| Publication/activation crashes | Last complete snapshot/root retained; incomplete objects rejected; no mixed active state |
| Rotation and key loss | Old backup remains readable with retained old key; new key behavior documented; no provider recovery fiction |
| Offline divergence and pruning | Both descendants preserved; newer deletion epoch blocks resurrection in active lineage |
| Index incompatibility | Local rebuild preserves record/vector provenance and reports actual restored/search readiness |

Marker scanning supplements, but does not replace, crypto verification. Local sync simulation proves object and ordering behavior; record an additional actual OneDrive handoff on two Windows environments for the declared provider support. Provider-side encryption is not counted toward I-19.

Add fixture checkpoints for encryption finalization, destination copy start/end,
full decrypt authentication, safe extraction, canonical import, activation intent,
pointer publication and new-root reopen. Provision B using the independently
supplied recovery copy and trust anchors; also create an unrelated wrong-key
identity for rejection tests. Cloning A's credential cache does not test recovery.
Include a correctly encrypted attacker-written archive, an authentic old snapshot,
a fresh-host unknown-head case and a prune racing a pinned backup. Compare both
the canonical exporter and vault filesystem observer with the expected fixture
inventory. Preserve raw local evidence only in disposable/ignored configured
locations and publish redacted summaries.

Record process-kill versus power-loss limitations, actual Windows/filesystem,
crypto/archive versions, backend, transferred sequence range and actual provider
handoff. A sync-folder simulator, marker scan or fake crypto adapter cannot pass
the omitted real campaign. The suite names below are planned interfaces until
their registered implementation targets exist.

Run `portability`, `store`, `recovery`, U04/M08 and applicable U05/U06/U09. Done when every cloud-bound byte follows the encryption boundary, recovery requires developer-controlled material, and a validated machine transfer preserves history/context/accounting without silent overwrite or authority transfer.
