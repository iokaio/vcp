# P3-06 local portability qualification

The production CLI and storage adapters passed the local checks below. Actual
U04 qualification remains pending: these runs used one Windows machine and local
ciphertext copies, not a two-Windows OneDrive transfer. Process termination checks
do not establish power-loss durability.

## Completed evidence

Logs are local, ignored artifacts under the repository's `artifacts/` directory;
they are not release inputs or portable trust anchors.

| Check | Observed result | Local evidence |
| --- | --- | --- |
| CLI library, qualification feature | 28 passed, including four automatic-trigger tests | `p306-cli-lib-final.log` |
| Complete CLI test targets, qualification feature | 63 passed, zero failed; three opt-in tests qualified separately | `p306-cancel-cli-final.log` |
| Qualified production executable | Build passed in 47.43 seconds | `p306-qualified-cli-build.log` |
| Actual CLI preview, restore, exact retry and explicit trust | Files and SQLite passed; 7.45 seconds reported by the native smoke run | `p306-cli-smoke.log`, `p306-cli-smoke.json` |
| Restore activation process termination | 12 positive cases passed: six durable boundaries in each cross-backend direction | Positive test in `p306-cli-restore-kills.log` |
| Receipt-before-selection tampering and deletion-floor changes | Six negative cases passed; focused rerun completed in 13.30 seconds | `p306-cli-restore-negative-kills.log` |
| Local A-to-B-to-A production CLI round trip | Files to SQLite to Files, and SQLite to Files to SQLite, passed with no provider requests | `p306-roundtrip.log`, `p306-roundtrip.json` |
| Existing active A root, descendant activation | Both backends passed; wrong expected descriptor rejected before materialization, correct selection preserved predecessor data | `p306-prior-root.log`, `p306-prior-root.json` |
| Native predecessor reopening after activation | Both retained roots reopened; workspace/task/session identities and recorded canonical/artifact hashes matched before and after reopening | `p306-predecessor-reopen-final.log` |
| Failed and restarted backup cancellation | Both backends passed; durable pins released, exact retry unchanged, no model attempts | `p306-cancel-host-final.log` |
| Reopened CLI cancellation without a key argument | Both backends passed; inactive jobs preserved, no vault deletion | `p306-cancel-offline.json` |
| Snapshot staging-root ownership | Six job tests passed; final focused both-backend retry also passed after adding inactive-history no-op coverage | `p5-09-job-root-fence.log`, `p306-staging-release-final.log` |
| Typed accounting and child state through encrypted restore | Both backend directions passed in one test; settled and unresolved amounts, child allocation and stable graph retained | `p5-10-portable-accounting.log` |
| Retained MiniLM vectors through encrypted restore | Both backend directions passed in one opt-in test; compatible vectors rebuilt locally without destination model calls | `p5-10-portable-vectors.log` |
| Restored lexical search | Both backends passed in one test | `p5-10-restore-lexical.log` |
| Owned vector staging cleanup | Passed; unexpected files preserved and cleanup reported pending | `p5-10-vector-cleanup.log` |
| Repository, harness and source provenance | All eight fast checks passed after correcting task-state vocabulary | `p306-fast-verified/a4aee9a9-f762-4a82-a75d-12f9901e7ce7/manifest.json` |

The first crash-test invocation passed the 12-case positive test but failed the
negative test's setup because its recovery-directory fixture omitted required
private-root constraints. After correcting that fixture, the negative test alone
passed all six cases. The first invocation as a whole was not green; the two logs
together provide the 18-case evidence.

The [native restore tests](../../src/crates/vcp-cli/tests/restore_crash.rs) kill the
actual qualified executable after intent, canonical import, activation receipt,
descriptor publication, trust advancement and rebind. Exact replay checks source
bytes, descriptor selection, canonical state, preserved original files and
Untrusted/Plan state. Negative cases change destination bytes, append canonical
state or independently advance the deletion floor after the activation receipt;
retry must refuse before selecting that destination. Qualification barriers are
inactive in an ordinary build.

The local round trips use real encrypted backup creation and restore. B adds
staged and unstaged changes plus new bytes, performs explicit rebind and trust,
and publishes a descendant snapshot. A independently enrolls at its previously
accepted parent checkpoint before restoring sequence 2. Assertions cover the
known parent digest, stable workspace/session/root-task identities and restored
dirty, untracked and B-added bytes. Local copies simulate transport only; the
reported cloud-transfer state remains unknown.

The existing-root campaign retains A's canonical and artifact bytes plus a local
workspace marker while selecting B's descendant into a separate root. The
ephemeral `owner.lock` token changes when the old store is opened for its required
ownership check; it is excluded from the canonical-data comparison.

## Boundaries exercised

[Crypto tests](../../src/crates/vcp-store/tests/vault_crypto.rs) cover the signed
`vcp-signed-age/1` envelope implemented with the qualified age 0.11.2 and
ed25519-dalek 2.2.0 primitives. [Snapshot jobs](../../src/crates/vcp-store/tests/snapshot_jobs.rs),
[restore staging](../../src/crates/vcp-store/tests/restore_stage.rs) and
[native checkpoint tests](../../src/crates/vcp-lifecycle/tests/support/backup_checkpoint.rs)
provide the underlying ciphertext, provenance and materialization fixtures.
The CLI campaign exercises these production boundaries rather than substituting
an archive mock.

[Portable retention](../../src/crates/vcp-memory/tests/portable_retention.rs)
passed on both backends in one 0.58-second test (`p5-portable-retention.log`):
a durable job pins retained payloads across restart and purge, stale publication
is rejected, cancellation releases its obligations, and a fresh post-purge
archive excludes the removed source marker. This is local retention evidence;
it does not prove deletion of independent external copies.

## Remaining qualification

Run the [two-Windows handoff procedure](p3-portability-handoff.md) on actual
OneDrive in both backend directions, with independent recovery material and
writer trust established before receiving ciphertext. The bounded collector
records redacted machine identity, object hashes and lengths, sequence/deletion
metadata and provider observations. Its local path/checksum/junction checks have
been exercised, but no actual OneDrive handoff is claimed. Access to the second
Windows environment is still pending; U04 and P3 completion must remain open
until that required evidence exists.
