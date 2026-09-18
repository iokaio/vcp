# ADR-019 — Cloud encryption and developer-controlled keys

Status: confirmed product direction; age 0.11.2 and Ed25519-dalek 2.2.0 selected as bounded P0 integration candidates. Production format and key UX remain unqualified.
Decision gate: P0-04/06, P3-06, P5-09/10, P8-03. Bounded P0 evidence is recorded below; production qualification and owner sign-off remain pending.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#1211-local-plaintext-encrypted-vaults-and-developer-keys) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Every cloud-bound VCP object and manifest is encrypted locally before publication; developer recovery secrets stay outside the vault. Active local files remain plaintext. Encryption failure blocks publication with no plaintext fallback.

## Implementation proposal

Separate key registry/recovery, local snapshot staging, encryption finalization, ciphertext-only publication and isolated restore. Pin developer-selected recipients outside project/restored configuration. Verify independent recovery before unattended backup. Authenticate writers separately from recipient encryption, then validate ancestry/deletion state before activation.

Detailed contracts and failure ordering are in the [supporting design](../architecture/storage-portability-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

The P0-04 comparison below selects existing age/Rust and Ed25519 implementations for further integration, with independent interoperability evidence. It does not establish a supported production writer-signature format or operational key ceremony. P5/P8 must qualify those contracts. Do not invent cryptographic primitives.

## Qualification evidence

P0-04 [prototype evidence](../evaluations/p0-04-portable-storage.md) qualifies existing age 0.11.2 and Ed25519-dalek 2.2.0 as experiment candidates, with Go age v1.3.2 and Node signature interoperability. Enroll writer keys and recovery recipients outside archive content; pin minimum sequence/deletion epoch and expected parent locally. Rotate recipients by re-encrypting objects and writers by explicit local trust replacement. No global freshness is inferred on a new offline machine. Pre-1.0 library status, production format review, protected keys and operator ceremonies remain P5/P8 work.

U04/M08/I-19 require wrong-key, header/payload tamper, truncation, untrusted writer, old replay, recovery-copy and rotation tests. Observe vault writes at intermediate/failure points; marker absence alone cannot prove encryption. Repeat with packaged binaries and two Windows environments.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Public-recipient knowledge does not establish authorized authorship. A fresh offline machine cannot prove global newest state without a trusted checkpoint; report that limit. Key loss cannot be repaired by provider encryption; rotation cannot erase previously copied ciphertext or secrets.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
