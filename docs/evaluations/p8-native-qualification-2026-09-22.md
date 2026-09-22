# P8 native qualification — 2026-09-22

P8-01 through P8-04 have an executable native matrix and an unsigned distribution candidate. The final matrix has **21 passing executable rows, zero failures and one not-run row** on the rebuilt candidate. This is partial qualification, not completion of the full P8 acceptance contract or owner approval.

## Tested environment and artifact

| Input | Observed value |
| --- | --- |
| Host | Windows 11 Pro, build 10.0.26200, x64 |
| CPU / RAM | AMD Threadripper PRO 5975WX, 64 logical CPUs, 137,295,024,128 bytes RAM |
| Shell / toolchain | PowerShell 7.6.6; Rust 1.98.0 MSVC; native ConPTY fixtures |
| Artifact | Unsigned Windows ZIP, debug qualification build, 73,811,665 bytes |
| ZIP SHA-256 | `f1838c6813bc3bfc6c085aa1e5e01b0e25fe36f1b3747e049e4d3624fb939189` |
| CLI SHA-256 | `50fef9aa0d598cd3a94c98bfbb2f7b25a07d917e30d9fd10fea50cd73ef07166` |
| Model provisioning | Ten pinned MiniLM files verified, 91,578,299 bytes; no download or provider call |

The candidate includes its payload inventory, build receipt, built-in skills, notices, standalone PowerShell installer, and explicit model provisioner. The exact archive was installed from its packaged installer into a fresh Unicode/space-containing directory, exercised with an empty profile and reduced process environment, and uninstalled. Workspace, history, local-key and vault sentinels remained byte-identical. This fresh environment is on the existing workstation, not a clean Windows installation or another OS account.

Native installer checks cover existing/unowned roots, redirects, persistent file locks, incompatible state metadata, interrupted upgrade, interrupted first install, malformed ZIP cleanup, wrong data-root recovery, validated rollback and preservation. A transient post-exit image lock prompted a bounded five-second retry; persistent locks still fail before removal. Inventory contracts accept 4,096 payloads and reject 4,097.

## Selected recovery and history matrix

| Area | Passing rows |
| --- | ---: |
| Native pause, same-process resume and hard close | 3 |
| Native policy revalidation | 1 |
| Exact packaged skill relocation, lazy loading and integrity | 1 |
| Graph, process, snapshot and restore recovery | 5 |
| Cleanup failure, retained references and receipt-publication interruption | 3 |
| Full binary history, compaction and structured review evidence | 3 |
| Ciphertext tampering, writer enrollment, recovery-key exclusion, rotation and native key handoff | 5 |

The new cleanup supervisor observes native removal, kills a real child process before receipt publication, reopens the canonical owner and reconciles the retained intent. Both Files and SQLite pass. A separate graceful hook-error case also passes; it is not presented as process-kill proof.

The initial matrix recorded 19 passes, two failures and one not-run. The exact-package row rejected an older executable digest after a rebuild. The store process-kill row correctly rejected Cargo's successful zero-test exit because its qualification feature was absent. A frozen follow-up enabled that feature and used the final candidate; both rows passed. Original failures remain retained. No source-integrity discrepancy occurred within either run.

Those retained manifests contain duplicate entries for two P8 runner inputs, and their Git metadata covers more evaluation scripts than their relevant-content comparison. The file digests still show stable relevant bytes; they do not establish a narrower Git metadata scope. The runner now deduplicates entries and uses one explicit scope for future captures. This metadata correction does not rewrite the earlier commands, logs or observations.

After a live P7 pause exposed a late response-callback race, the lifecycle was corrected and the CLI rebuilt. The final full matrix reran all 21 executable rows with the corrected source identity and the exact candidate above: all passed, with the same independent-machine gap. The final archive also passed a fresh install/CLI/uninstall smoke run. Late normal/error callbacks after retained cancellation have a separate both-store regression; real capture capacity failures still fence transport. These newer observations supersede the initial candidate where the shared lifecycle changed.

Reproduction is described in [P8 readiness](p8-qualification-readiness.md) and [distribution operations](../development/p8-distribution.md). Final evidence is retained under `artifacts/p8-post-pause-matrix`, `artifacts/p8-post-pause-smoke.log` and the exact-package result directories. Initial runs remain under `artifacts/p8-owner-matrix`, `artifacts/p8-owner-matrix-corrections` and `artifacts/p8-owner-selected-evidence.json`. These contain command/runner/source identities and hashed logs; private machine paths and full outputs are not published.

## CPU measurement

A fresh current-source `vcp-embedding` qualification build passed all four golden reference cases and four checks in each of three independent processes. Wall times were 5,906 / 5,507 / 5,512 ms; sampled peak working sets were 232,579,072 / 220,286,976 / 219,611,136 bytes. The public pinned assets were already present. The binary and build-log identities are retained in `artifacts/p8-current-cpu/result.json`.

This measures the CPU embedding subsystem on the stated high-memory workstation. It does not establish minimum supported hardware, a packaged end-to-end resource envelope, or OS-enforced network denial. Older AppContainer receipts are retained separately and are not substituted for current-source measurements.

## Open gates

The required packaged second-machine encrypted recovery row is **not run**. The current account cannot administer the available Hyper-V service, and no independent recovery fixture is available. Native key handoff between local directories is useful component evidence, not a second-machine result.

Clean-OS install qualification, full packaged U01–U09 integration, the broader fault schedule, P8-06 upstream maintenance rehearsal and P8-05 human owner acceptance remain open. No cross-format store migration, signing, release publication or deployment is claimed. The installer refuses incompatible declared formats; a future cross-format upgrade requires a separately validated snapshot and staged restore.
