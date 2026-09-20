# P5-09 / P5-10 storage qualification

This records the native Windows storage evidence for encrypted publication and staged restore. It is a foundation qualification, not a declaration that all P5-09, P5-10 or P3-06 host and CLI acceptance conditions are complete. The binding admission and retention rules remain in [storage portability design](../architecture/storage-portability-design.md) and [plan 11](../plan/11-encrypted-portability.md).

## Boundaries implemented

Developer-controlled recovery copies are verified before enrollment. Recipient and writer trust live in an independently selected, owner-locked local journal; decrypted archives cannot enroll writers, replace local trust, supply credentials or create live execution permits. Private key material is neither passed in process arguments nor serialized into canonical records. The encrypted envelope contains the signed inventory and all payloads. Publication accepts an opaque finalized ciphertext capability and creates only a new ciphertext object in the vault.

Snapshot jobs record the exact canonical cut, artifact and generation pins, source root, authority and deletion epochs, input identity and stages before lengthy I/O. Native workspace checkpoints retain staged, dirty and untracked bytes within explicit bounds. Generation components carry direct source provenance from their first canonical insertion, so later erasure discovers copies even if job creation was interrupted. Preserved derivatives require destination reopening; imported canonical history alone never establishes search readiness.

Input validation and retained-ciphertext hashing have opaque preparation APIs for blocking workers. Canonical admission consumes the proof only after checking the exact captured root and watermark, or the exact current job, trust configuration and workspace epochs. A stale proof cannot authorize a new copy. The compatibility wrappers perform both phases synchronously and are intended for callers that already run outside an interactive owner.

Durable `Admitted` is the final publication admission boundary. A copy admitted before a purge can finish afterward and remains an explicit backup-copy obligation. New admission after the epoch changes fails. Releasing source pins does not erase the published job, its ciphertext identity or its retained-copy obligation. Copy ownership is recorded using native file identity before bytes are written. Unknown files and the create-before-ownership-receipt crash gap remain conflicts; recovery does not guess ownership or delete foreign files.

Restore authenticates against independent trust before importing a neutral canonical archive. It preserves historical audit and receipt facts, then appends one authority reset: workspace authority and binding advance, trust becomes untrusted, grants are revoked, policies lose automatic execution authority and unfinished tasks become paused. An opaque imported result requires a fresh exact-state and artifact verification before activation. Existing-root selection is a host responsibility; a plain canonical directory is never passed to the unrelated migration initializer.

After explicit restore and ordinary rebind, `restore_search::rebuild_after_restore` rebuilds and reopens a lexical generation from the currently authorized canonical inventory without loading a model. It uses the shared local CPU admission pool and retains a CPU/RAM/disk observation. Matching current generations reopen on retry without a new receipt or generation. Readiness separates usable lexical coverage from semantic lag and stale retained-source manifests: rebind does not make old native observations current. Source coverage requires deliberate trusted recapture, while the rebuilt index can already serve eligible retained claims. This operation does not grant trust, resume tasks or read arbitrary workspace files.

Workspace installation uses a newly created native staging directory and create-only whole-directory rename. It preserves existing empty directories, nonempty directories and files. Parent handles remain pinned, and the stage native identity is checked when acquiring the rename handle and again at the destination. Failed or ambiguous operations preserve their files for explicit reconciliation.

## Reproducible native checks

Commands ran with stable Rust, existing offline dependencies, no explicit target triple, and `CARGO_TARGET_DIR=artifacts/p5-vault-target`, from `src/third_party/codex/codex-rs` after loading `artifacts/p5-native-env.ps1`.

| Command suffix after `cargo +stable` | Result |
| --- | --- |
| `test --offline --locked -j4 -p vcp-store --test vault_crypto --test key_publication --test trust_store -- --nocapture` | 12 passed; independent interop test separately qualified |
| `test --offline --locked -j4 -p vcp-store --test key_publication independent_age_and_node -- --include-ignored --nocapture` | 1 passed with the pinned age v1.3.2 executable and Node |
| `test --offline --locked -j4 -p vcp-store --test restore_stage --test snapshot_jobs -- --nocapture` | 9 passed, including 28 actual child-process kills across both backends |
| `test --offline --locked -j4 -p vcp-repository --test restore -- --nocapture` | 3 passed: whole-directory publication, all destination collision kinds, replacement and junction rejection |
| `test --offline --locked -j4 -p vcp-memory --test portable_retention -- --nocapture` | 2 passed on both backends: unadmitted stale proof rejected; preadmitted copy completes while retaining its backup obligation |
| `test --offline --locked -j4 -p vcp-store --test snapshot_jobs -- --nocapture` | 6 passed after preparation split, including cancelled input validation and stale input-cut rejection |
| `test --offline --locked -j4 -p vcp-lifecycle --test canonical_host restore_search:: -- --test-threads=1 --nocapture` | 1 passed across both backends: actual retained-preference lexical query, stale-source exclusion, unchanged Untrusted/Paused state, successful token reuse and generation retry without writes |

The snapshot process-kill fixture covers captured, archive prepared, encrypted, admitted, copy identity retained, partial copy, copied and completed barriers on Files and SQLite. Restore kills cover acquired, validated, import intent, partial spool, sanitized and imported barriers in both cross-backend directions. These are fresh-process recovery tests, not only in-memory stage simulations.

Strict repository Clippy passed for the native staging module and test. Store Clippy passed for the library and snapshot/restore tests with only the two existing unrelated lint allowances (`manual_is_multiple_of`, `cmp_owned`). Logs are retained locally under `artifacts/p5-09-*` and `artifacts/p5-10-*`; they contain test results rather than recovery secrets. Host capture/materialization and CLI delivery evidence are tracked separately in [portable capture qualification](p5-portable-capture.md).

The focused post-restore lifecycle library Clippy run (`clippy --offline --locked -j4 -p vcp-lifecycle --lib --no-deps -- -D warnings`) reported no diagnostic in the new helper, but failed on six existing `too_many_arguments` process-boundary functions. No lint allowance or unrelated process refactor was added for this increment; the integrated final gate remains outstanding. Its log is `artifacts/p5-10-restore-lexical-clippy.log`.

## Explicit limits and remaining integration

The neutral archive currently bounds retained raw payloads to 4 MiB and 4,096 parts; private archive encoding is bounded to 16 MiB. Crypto also has explicit object, plaintext and ciphertext limits. Larger inputs fail visibly instead of reporting a complete backup. Declared private and sync roots are checked; this is not universal sync-root discovery or protection from a hostile process running as the same user.

A job still at `Captured` when a physical source-root rewrite occurs requires retained-source reconciliation; it does not silently substitute the newer cut. Prepared and later stages have durable retry evidence. Git metadata is retained as historical input, not installed or executed as repository setup. Preserved search files remain `ReopenRequired` until the destination validates the actual engines and specifications.

The evidence covers process termination and ordinary file-flush/reopen behavior. It does not prove hardware power-loss durability, cloud synchronization acknowledgment or knowledge of a globally newest copy. Exact-known-head verification is distinct from accepting a newer restore and never lowers a replay floor. Overall completion still requires the host/CLI activation, configured trigger, liability, handoff and required CI checks recorded by the owning work items.
