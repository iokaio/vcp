# P1 foundation increment qualification

On September 18, 2026, the first P1 library increment passed 24 native contracts
on Windows 10.0.26200 / NTFS, using Rust 1.98.0
(`88d9e12ae`, August 18, 2026) and MSVC 14.50.35717. The
[implementation guide](../development/p1-foundation.md) records formats,
commands, limits and remaining integration. P1-01 through P1-04 remain
`in_progress`; this report does not claim the entire P1 phase is complete.

Executed command: `pwsh -NoProfile -File scripts/test-foundation.ps1`, exit 0.
The runner verified committed-source reconstruction identity, ran the four real
Rust packages with locked offline dependencies, checked all 24 named contracts
and verified source inputs stayed unchanged throughout qualification.

| Boundary | Observed result |
|---|---|
| Domain and protocol | Generated state sequences reject unsupported completion; fresh fingerprints/steering govern evidence; full-width counters round-trip; JSON key order preserves command meaning |
| Commands | Interactive/JSONL calls return the same durable result; restart retries do not duplicate events; current revocation blocks old receipts; questions preserve objectives; paused parents preserve children and forks retain independent state |
| Decisions/subscriptions | Actor/hash/revision/steering/expiry binding rejects stale decisions; bounded pull pages retain exclusive sequence ordering and captured snapshot boundaries |
| Capture | Binary streams beyond UI-sized tails, empty streams, stdout/stderr separation, aborts, interrupted prefixes, finalize failure, corrupted chunks and credential/configuration separation behave explicitly |
| Shared backends | Atomic records/events/receipts, scoped references, revisions, writer collision, pinned artifacts, orphan collection and bidirectional conversion agree |
| Independent transaction crashes | Eight real process kills: before preparation/commit and after commit/before reply, with independent SQLite row or journal-marker observations and idempotent reopen |
| Controlled activation | Six process kills around validation/activation leave one recognized active backend and preserve the old recovery root |
| Storage faults | Bounded SQLite contention produces no acknowledgement; a handle with an uncertain write requires reopen; uncommitted file tails are quarantined while committed corruption/truncation fails closed |

During development, native testing exposed a SQLite WAL metadata race during
reopen. SQLite's closing worker can remove its own WAL/SHM files; the corrected
preflight accepts absence while rejecting observed links and other I/O errors.
The subsequent full qualification passed. These tests exercise forced process
failure and synced file contents. Hardware power loss, other filesystems and
installation of a final VCP package were not tested.

The ignored local evidence run is
`artifacts/foundation/bc141414-28e2-4995-b8fc-7d3ef77283e5/`.
Its manifest SHA-256 is
`8430a78346d09c1ab9f22df9448502fd2ce1085b6f38a4c104587357f0a3f364`;
the native contract log SHA-256 is
`4eb63af3d5a35a6215f4d93bf73b5653c524c3140294209bf2220500b12b2b22`.
The manifest includes every tested original source/script hash; the report
does not require private inputs or raw local paths for reproduction.

Independent reconstruction from the immutable Codex selection plus patches
0001–0011 reproduced all 7,938 committed files, with aggregate file hash
`4c09f835cdfc11bcdeaf286286feba8d3cd92118e830f87ea490030522ddf2fe`.
Patch 0011 only connects original VCP packages to the existing workspace and
lockfile; all external dependency entries remain byte-equivalent. Cargo metadata
independently confirms 165 workspace packages, matching the expanded explicit
boundary inventory (28 groups and 58 named seams).

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all eight cases,
including repository links, harness contracts, source reconstruction and explicit
package ownership. Local run: `70ee7e8c-1bf0-4777-8ea9-86d683297b23`. An earlier
run correctly rejected the newly added packages until their explicit boundary
roots and owners were recorded; the gate was preserved.

Remaining P1 acceptance includes the root/child ledger and accounting constraints,
fresh-process history projection rebuild, filtered history/retention cursors,
and full capture/accounting through the retained controller and helper transport
paths. No paid provider calls or hosted Windows qualification were used here.
