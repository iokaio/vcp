# ADR-019 — Cloud encryption and developer-controlled keys

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P0-04/06, P3-06, P5-09/10, P8-03. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#1211-local-plaintext-encrypted-vaults-and-developer-keys) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Every cloud-bound VCP object and manifest is encrypted locally before publication; developer recovery secrets stay outside the vault. Active local files remain plaintext. Encryption failure blocks publication with no plaintext fallback.

## Implementation proposal

Separate key registry/recovery, local snapshot staging, encryption finalization, ciphertext-only publication and isolated restore. Pin developer-selected recipients outside project/restored configuration. Verify independent recovery before unattended backup. Authenticate writers separately from recipient encryption, then validate ancestry/deletion state before activation.

Detailed contracts and failure ordering are in the [supporting design](../architecture/storage-portability-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

The age/Rust approach is a candidate, not an accepted library/version or writer-signature format. P0-04 must compare existing supported implementations, record independent interoperability and choose writer enrollment, revocation/rotation and replay handling. Do not invent cryptographic primitives.

## Qualification evidence

U04/M08/I-19 require wrong-key, header/payload tamper, truncation, untrusted writer, old replay, recovery-copy and rotation tests. Observe vault writes at intermediate/failure points; marker absence alone cannot prove encryption. Repeat with packaged binaries and two Windows environments.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Public-recipient knowledge does not establish authorized authorship. A fresh offline machine cannot prove global newest state without a trusted checkpoint; report that limit. Key loss cannot be repaired by provider encryption; rotation cannot erase previously copied ciphertext or secrets.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
