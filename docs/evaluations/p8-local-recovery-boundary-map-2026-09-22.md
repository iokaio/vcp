# P8-02 local recovery boundary map — 2026-09-22

This records the evidence audit performed before the bounded recovery increment.
It preserves passing observations and the gaps identified at that time. The
[increment report](p8-local-recovery-increment-2026-09-22.md) records subsequent
commands, outcomes and remaining limits separately; its results do not rewrite
this pre-run baseline. This map does not complete P8-02. Machine handoff remains
owner-skipped. The
[acceptance reconciliation](p8-acceptance-gaps-2026-09-22.md) continues to own the
broader qualification limits.

## Receipts inspected

The current broad-run manifest is
`artifacts/p8-local-matrices/e21f5098-a79b-4ebb-8d6a-6d469c91ee7d/matrix/manifest.json`,
SHA-256 `9077462efcb84703d26b166884dde25d8e33f76af6d31d5f656275805e6fcaf0`.
All sixteen P8-02 rows have `pass`, exit zero and the expected exact test observed.
The audit recomputed all 48 referenced command/stdout/stderr hashes successfully.
Seven test-source files used for the crash-boundary mapping below also matched
that manifest's source inventory before the increment added new tests. This is
not an assertion that later edited files still match the historical inventory.
These are completed individual
rows from an incomplete broad campaign, not a passing full campaign.

The focused manifest is
`artifacts/p8-local-supplements/fd3354b5-39b5-4538-98b8-0c9ba1108631/matrix/manifest.json`,
SHA-256 `3efc9c9a4ceaf503bb8eb7f8795265a7daa94a0583f2b72a9b52ea2acb90a188`.
Its packaged long-check pause row has a complete passing exit receipt; all three
referenced file hashes were also checked. The independent-machine row and
unreceipted interrupted attempts are not promoted to passes.

The [current-machine report](p8-local-qualification-2026-09-22.md) identifies the
debug qualification package used by packaged rows. Native Rust test executables
are component/host evidence, not production-package evidence. Registration in
`scripts/evals/p8-qualification-manifest.json` alone is not execution evidence.

## Executed crash schedule

Every row in this crash-schedule table uses both Files and SQLite. “Owner”
identifies the store or controller process under test, not a delegated VCP child.
Each schedule is one
deterministic fixture traversal per backend; no randomized seed or repeated-seed
campaign was recorded. Fresh generated IDs are not repeat seeds. Boundary names
come from the source matched to the receipt, while the retained logs record the
enclosing exact test result. Temporary per-barrier roots/markers are not retained
as separate durable receipt files by these older tests.

| Existing P8 case | Exact boundary and position | Role / variants | Reopen oracle | Observed command seconds |
|---|---|---|---|---:|
| `p8-02-store-process-kill` | `prepared`, `before_commit`, `after_commit`, `before_reply`: eight kills total | Store owner; one fixed transaction | Independent SQLite table counts or Files commit marker, acknowledged watermark, original idempotent receipt, real writer lock released | 16.618 |
| `p8-02-snapshot-job-recovery` | After `captured`, `prepared`, `encrypted`, `admitted`, `owned`, `partial`, `copied`, `completed`: sixteen kills total | Root snapshot job; sequential recovery of the same job per backend | Published active job, one vault object, captured watermark and unchanged source task revision | 19.919 |
| `p8-02-restore-process-recovery` | After `acquired`, `validated`, `intent`, `partial`, `sanitized`, `imported`: twelve kills, followed by two normal `finish` exits | Local restore owner; Files→SQLite and SQLite→Files, sequential recovery of the same import | Imported target watermark three, one authority reset, untrusted workspace, durable `Stage::Imported` | 9.586 |
| `p8-02-child-cleanup-receipt-publish-interruption` | `after_native_removal_before_receipt_publication`: two kills | Parent controller managing a registered child's disposable root | Observed removal, retained cleanup intent, reconciliation after reopening both stores | 6.631 |
| `p8-02-integration-receipt-kill` | `native_write_before_receipt` after write index zero and one: four kills | Parent integration of isolated child output | Partial effects, later human edits, history and accounting preserved without replay | 16.760 |
| `p8-02-child-write-receipt-kill` | `child_native_write_before_receipt`: two kills | Registered child native write; parent/sibling isolation | Unknown effect, first write, later human edit, transcript and accounting survive without replay | 10.665 |
| `p8-02-real-generation-kills` | Before/after each of lexical, vector, manifest and activation: sixteen kills | Store owner publishing a derived generation | Previous generation except after activation, then newly published generation; complete vectors and canonical ready intents | 46.715 |

Snapshot `owned` follows durable copy-identity recording; `partial` is inside
ciphertext-byte publication; `copied` precedes canonical job completion. This
already supplies local vault-copy interruption evidence. It does not establish
cloud hydration or provider transfer. Generation activation is a derived-index
pointer change; restore import is not active-root activation. Those boundaries
must not be treated as interchangeable.

The test sources are
[store transaction/migration](../../src/crates/vcp-store/tests/process_recovery.rs),
[snapshot jobs](../../src/crates/vcp-store/tests/snapshot_jobs.rs),
[restore staging](../../src/crates/vcp-store/tests/restore_stage.rs),
[generation publication](../../src/crates/vcp-memory/tests/publication.rs),
[cleanup receipts](../../src/crates/vcp-lifecycle/tests/support/child_cleanup_receipt_fault.rs),
[integration writes](../../src/crates/vcp-lifecycle/tests/support/child_integration_fault.rs)
and [child writes](../../src/crates/vcp-lifecycle/tests/support/child_write_crash.rs).

## Other passing recovery observations

These rows retain useful evidence but do not add unspecified process-kill
positions to the schedule above.

| Existing P8 case | Observed scope | Seconds |
|---|---|---:|
| `p8-02-child-graph-both-backends` | Atomic bounded child registration and durable graph | 22.906 |
| `p8-02-fresh-process-history` | Fresh-process unknown native process, paused child and late charge | 7.911 |
| `p8-02-child-cleanup-faults` | Native partial removal failure with retry intent/ownership marker; repository fixture, no store-backend dimension | 16.261 |
| `p8-02-child-cleanup-retention-references` | Both-store retained cleanup intent/results/diagnostics; only original owned root reconciled | 6.512 |
| `p8-02-child-independent-stop` | Calling sibling and exact liability preserved on individual child cancellation | 29.213 |
| `p8-02-child-output-consumer-loss` | Noisy/quiet registered children survive consumer loss and cursor recovery | 29.268 |
| `p8-02-real-vector-pause` | Actual local model work pauses/cancels without false readiness | 11.753 |
| `p8-02-real-publication` | Real lexical/vector generation adopted with exact canonical intent coverage | 10.108 |
| `p8-02-retained-vector-restore` | Opposite-backend encrypted restore retains vectors without loading a destination model; excludes stale sources and reopens paused/untrusted authority | 14.343 |
| `p8-02-packaged-long-check-pause` | Both-store debug package pause after independent process start; held process handle proves termination, no passing check or replay | 48.558 |

## Historical restore activation evidence retained

[P3-06 portability qualification](../development/p3-portability-qualification.md)
already records actual qualified CLI termination after `intent`, `import`,
`activation_receipt`, `descriptor`, `trust` and `rebind` in both cross-backend
directions: twelve positive arms. These are root restore operations, using the
fixed Files/SQLite encrypted fixture seeds once per phase. Exact retries preserve
canonical/source bytes and Untrusted/Plan state. A separate six-arm negative
schedule changes source bytes, canonical state or the current deletion floor
after the activation receipt and requires refusal before selection.

The retained positive log was read directly:
`artifacts/p306-cli-restore-kills.log`, SHA-256
`00e1d25aac1499e3982eb43e635213e1976bba07de08a51155835c4b073bdc09`.
Its positive exact test passes, but the enclosing run fails on the negative
test's fixture setup. The focused repaired negative log,
`artifacts/p306-cli-restore-negative-kills.log`, SHA-256
`1edfd18f0962f3581df08f521d1f35a0cdc4c9ca334e31394ec6ff870736d0d5`,
records one passing exact test in 13.30 seconds. The initial failure remains a
failure; these are distinct historical observations.

P3 also records both-backend existing-active-root descendant activation and
predecessor reopening, separately from the kill schedule. Its retained result
`artifacts/p306-prior-root.json` has SHA-256
`855bea7d702986d027c3d68d93b9d4991c83cf69dd96163fcfa5fdf4f35d9450`.
Thus the missing combination is interruption **with an existing active
predecessor**, not an absence of all restore-activation crash evidence. Preserve
the existing positive/negative schedules instead of repeating them wholesale.
Historical binaries and local logs are not relabeled as the current production
package or current P8 command receipts.

## Missing local cases selected first

| Gap at audit time | Existing implementation/test evidence | Required bounded increment |
|---|---|---|
| Interrupted restore activation with old and new roots present | The current P8 restore row stops at imported state. Historical P3 six-phase activation kills and existing-predecessor activation pass separately, as detailed above. The source also has a migration-only activation kill test; that is a different operation. | Combine an existing active predecessor with actual restore activation interruption, both backend directions. Keep both roots; reopen must select the validated recorded root, preserve acknowledged old-root facts and imported history/liabilities, and avoid repeating authority reset. |
| Disk exhaustion | No explicit disk-full/ENOSPC/StorageFull qualification was found in the inspected store/memory tests or P8 manifest. [P1 qualification](p1-completion.md) explicitly limits spool-capacity faults: these do not prove physical disk-full behavior. | Inject a genuine bounded storage-full error at the claimed write surface, or label a deterministic injected equivalent precisely. Preserve prior acknowledged data, reject acknowledgement of incomplete work, fence uncertain writes and prove reopen/retry after capacity returns. Never fill the workstation volume. Distinguish canonical writes, artifact finalization and ciphertext/restore staging; one surface does not qualify all. |
| Canonical versus derived corruption on reopen | `committed_corruption_fails_closed_and_only_partial_tail_is_quarantined` and `truncating_an_acknowledged_file_commit_is_corruption_not_a_recoverable_tail` cover Files in source. `publication_cas_retry_corrupt_fallback_and_pins_preserve_canonical_evidence` covers derived lexical corruption on both stores, but does not itself close/reopen the store after corruption. None is a named P8 aggregate row. | Obtain explicit current receipts for fail-closed canonical corruption on both stores, distinct from uncommitted-tail recovery. Corrupt/remove derived generation data, reopen and demonstrate fallback/rebuild with canonical records, receipts, references and liabilities unchanged; record search visibility while rebuilding. |

The bounded runner must declare an absolute deadline before every selected
command, terminate its owned process tree on expiry, and retain timeout/exit/log
receipts. A barrier's existing 20- or 30-second polling timeout is not a deadline
for compilation, the complete test or a blocked native child wait. Run only the
missing cases and affected regressions; keep earlier passed receipts intact.

The selected implementation approach is SQLite's real `max_page_count`
`SQLITE_FULL` failure, a precisely labeled test-only Files-journal `StorageFull`
injection, and a new fresh-reopen canonical/derived corruption comparison. The
Files injection is not physical-media exhaustion; SQLite's page limit does not
establish whole-volume exhaustion. These describe the increment selected from
this audit. The [increment report](p8-local-recovery-increment-2026-09-22.md)
owns its execution receipts and disposition; this paragraph makes no passing
result claim.

Artifact-finalization crash positions, all root/child model-dispatch positions,
prune tombstone/cleanup positions, selected combined faults, full trust-boundary
variants, cloud hydration/offline divergence, and retained per-barrier supervisor
receipts remain outside what this audit establishes. Machine handoff stays
skipped. Production package qualification and P8-03/P8-05 remain separate work;
this map does not waive their acceptance conditions.
