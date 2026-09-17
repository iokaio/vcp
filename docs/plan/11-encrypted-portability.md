# 11 — Developer keys, encrypted backups and machine handoff

Status: planned. Owns P5-09, P5-10 and P3-06. Requires qualified P0 encrypted-storage/key/writer-trust decisions, stable store/generations/pruning and workspace continuation. Architecture sections 12.8–12.11 and ADR-019 are binding.

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

## P5-10 — Restore and sequential handoff

1. Hydrate/download encrypted objects into local staging. Supply the developer's independent recovery identity; authenticate/decrypt the full stream including final-chunk detection before activation.
2. Verify writer trust, inner inventory, bounds, archive paths/link behavior, hashes, schema versions, source references, generation compatibility, deletion epoch and ancestry. Reject encrypted-but-untrusted new-writer content and valid older replay according to the chosen trust/lineage contract.
3. Extract into an isolated plaintext local root. Apply resource/expansion limits and ensure no archive entry escapes it. Wrong/missing keys or any failed validation preserve the active environment unchanged.
4. Rebind workspace roots, local credentials/tool dependencies and authority; restore liabilities and unfinished children. Open compatible indexes or rebuild locally from retained sources/vectors while retaining provenance and exposing progress.
5. Activate the validated root through a crash-tested pointer/switch protocol. Preserve the last valid root until the new one reopens successfully. Backend conversion uses the same neutral record contract and validation.
6. For A-to-B-to-A handoff, preserve stable workspace/task IDs and snapshot parent relationships. Offline descendants remain separate until explicit branch selection or supported reconciliation. No filename timestamp merge or cloud-wide writer-lock claim is permitted.

Rotation creates/verifies a new identity before selecting it for future snapshots. Old snapshots require their old identities unless locally re-encrypted into new verified bundles. Losing all recovery keys makes those cloud snapshots unrecoverable; surviving plaintext local data can start a new encrypted lineage. Rotation cannot retract already obtained keys/ciphertexts.

## P3-06 — User commands and diagnostics

Implement `storage configure/migrate --preview`, `backup keys create/import/verify/rotate`, `backup configure/create/status`, `restore --workspace --key`, and `workspace rebind`. These commands orchestrate the services above; never place literal secret material in argv or ordinary JSONL.

Show unsynced sequence range, selected key reference, independent-recovery verification, missing tools/secrets and whether a snapshot was merely published or actually restored. Without decrypted local metadata, browse opaque vault objects only. Local exports may be plaintext outside sync roots; cloud-directed exports use the same publisher. `doctor` checks path/junction separation without claiming to detect every unrelated synchronization program.

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

Run `portability`, `store`, `recovery`, U04/M08 and applicable U05/U06/U09. Done when every cloud-bound byte follows the encryption boundary, recovery requires developer-controlled material, and a validated machine transfer preserves history/context/accounting without silent overwrite or authority transfer.
