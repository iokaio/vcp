# ADR-021 — Sealed replay bases for retained-content rewrites

Status: accepted for the P5-07 storage foundation; retention planning and physical
cleanup remain separate work.

## Context

Version-1 canonical roots replay all historical transaction bodies. Files
checkpoints are checked against that history, and SQLite also materializes copies
of records, events and command results. A logical tombstone cannot remove content
from those containers. Replaying original transactions during backend conversion
would restore the content. Artifact-only snapshot pins also omit content embedded
in the canonical state.

## Decision

An explicit rewrite creates a new, contained physical root with a bounded sealed
`replay-base.json`. Its version-1 envelope contains the validated replacement
state, its watermark and a commitment to the source state. Its separate seal
checks its exact bytes. Format-2 roots require this base; existing format-1 roots
continue to open without migration. This does not change the version-1 commit
wire format.

The base preserves historical transaction IDs, request digests, watermarks and
minimal receipts. These digests are commitments to historical request bodies;
the store does not claim to possess or reconstruct those bodies. Explicit
redaction may replace content-bearing inspection results while preserving both
receipt copies consistently. New transactions continue at the next watermark.
Exact retries consult the retained commitments and do not replay their effects.

Files journals begin their suffix chain at the base seal. Checkpoints and prefix
validation begin at its watermark. SQLite schema 2 materializes the base before
accepting suffix commits; old event/command watermarks need not reference retained
commit bodies. Canonical state validation continues to validate their identities
and relations. A newly built database avoids inheriting old free pages, WAL or
historical commit payloads. Conversion copies the replay boundary plus its suffix,
not the erased prefix. Pre-base content queries fail explicitly. Outer migration
anchors retain their already-validated prefix digest as an opaque commitment;
this permits reopening the migration manager without reconstructing erased
historical content.

Only exact typed erasures are admitted at the rewrite seam. Purged artifacts
retain truthful identity/hash/length metadata and explicitly lack retained bytes.
Redacted claims, events and terminal task content are distinct representations,
not invented normal values. Ordinary transaction admission cannot introduce them.
Unsettled accounting and active recovery remain protected. The controller still
owns selection, authorization, tombstone commit and dispatch ordering.

`Store::open` resolves an immutable, hash-linked activation record at the original
root into a unique `.replay-roots` child. Activation occurs only after a fresh
open validates the candidate. The original anchor owner lock stays held, so old
workspace descriptors cannot accidentally create a second active writer. Missing
or corrupt activated roots fail closed; there is no implicit fallback to old
content. Same-backend rewrites use this activation path.

Snapshots hold a root-level lease in addition to artifact leases. Old physical
roots and snapshots remain explicit pending retention obligations after
activation. The recognized-file inventory is bounded and rejects unknown or
redirected entries; it is not deletion authority. This increment never deletes
old roots, claims physical cleanup is complete, or promises removal from SSD
blocks or external backups. Future cleanup must acquire the root/artifact leases,
recheck durable pins, and preserve anchor routing records and current children.

## Evidence

The store `replay_base` integration suite exercises both backends: preserved
receipt retries, suffix/checkpoint reopen, conversion, repeated activation,
artifact copying, root snapshot leases, corrupt base refusal, and real child
process exit immediately before/after activation. A synthetic content fixture
checks task/event/artifact marker absence separately in the replacement root and
new snapshot, while proving an old snapshot/root still retains an explicit
pending obligation. This is foundation evidence, not full P5-07 acceptance.
