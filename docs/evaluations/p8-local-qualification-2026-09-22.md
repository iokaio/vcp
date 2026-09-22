# P8 current-machine qualification — 2026-09-22

This continuation covers P8-01 through P8-04 on the existing Windows workstation.
The owner explicitly directed us to skip machine handoff and make the best
available local validation. Fresh directories, isolated profiles and local
cross-backend restore are reported as such; independent-machine recovery and a
clean Windows installation are not run. This is an unsigned qualification
candidate, not owner release acceptance or publication.

## Host and execution scope

The host is Windows 11 Pro, build 26200, x64, NTFS, with an AMD Ryzen Threadripper
PRO 5975WX (32 cores/64 logical processors) and 137,295,024,128 bytes of physical
RAM. PowerShell is 7.6.6, Node is 24.21.0 and Git is 2.43.0.windows.1. Native
qualification uses Rust 1.95.0 and the installed Visual C++ tools, with two Cargo
jobs and a serialized shared cache. This host does not establish minimum CPU or
RAM requirements. No paid provider requests are part of this continuation.

Environment metadata is retained at
`artifacts/p8-local-environment/environment.json`. Full artifacts remain ignored;
recovery keys and decrypted fixtures are not distribution inputs.

## Findings corrected during qualification

The package script supplied a backslash-form path to Git's exact
`safe.directory` setting. Git did not recognize that spelling in the delivery
worktree. Both calls now normalize the path to forward slashes. The existing
native installer contract passed without an environment trust override after
that correction: interrupted install/upgrade, rollback, ownership checks,
locked uninstall recovery, junction refusal and protected-data preservation.
These are synthetic payload tests, separate from exact-executable smoke tests.

The passing installer receipt is
`artifacts/p8-local-installer-contract/96b08bae-eed1-46b2-a2ee-ec79fd45217d/result.json`,
SHA256 `3693ec8f13932713e995977dcd33375f5653bf0f0399d6ed9bf0d60cd352f2d9`.
Execution took 17.794 seconds; log SHA256 is
`05683065e7f20a2998551b51052c81294746de2b421f51fd3bb0ae1c007a76ae`.
The original failure and interim environment-workaround run remain preserved.

The maintenance full-host run exposed a second defect: deliberately launching a
malformed executable opened Windows' “Unsupported 16-Bit Application” dialog.
That blocked native creation before the ordinary process deadline could start.
Closing two test-owned dialogs allowed the original assertions to finish; this
manual intervention is retained and is not an unattended passing result.

Direct, duplex and PTY native creation now use a synchronous, thread-local
critical-error suppression scope. It preserves other error-mode flags and
restores the prior mode on return, error or unwind, without changing process-wide
settings or execution authority. This follows Microsoft's
[thread error-mode contract](https://learn.microsoft.com/en-us/windows/win32/api/errhandlingapi/nf-errhandlingapi-setthreaderrormode).
A supervised malformed-image regression requires the native format error and a
completion marker within a bounded deadline. The existing both-store verification
case still requires unknown dispatch evidence and refuses completion.

The P8 runner also now assigns a fresh metadata receipt path to each case and
retains the packaged crypto receipt's digest. A regression verifies that a stale
inherited path is neither reused nor overwritten.

The first five-case packaged run passed independent encryption/restore and
failed four cases. Its receipt and diagnostics remain at
`artifacts/p8-local-package-tests/3c01fa73-1eba-4b50-b45b-b8dfc692fb76/`.
The long-check fixture supplied unsupported public environment variables;
marker paths now appear as JSON literals in the fixture's source, preserving
the existing process environment policy. The history fixture incorrectly
required no gap annotations: ordinary captured output deliberately records
authentication-header and recovery-material exclusions. Qualification must
check those exact annotations as well as complete ordinary output bytes.
MCP credential resolution also exposed a product issue: configuration errors
occurred after task acceptance and lost their safe, specific diagnostic.
The CLI now resolves the explicitly configured references once before accepting
a task. Validated, zeroizing material stays in memory until installation under
the canonical owner; expiry still starts at installation. Missing/invalid
credentials return the specific safe configuration error before any accepted
event, provider request or MCP connection. No raw downstream error is exposed.
Fresh evidence below supersedes this failed run only where explicitly stated.

## Qualification status

The fast delivery suite passed all 17 groups, including the runner/oracle and
distribution contracts. Receipt:
`artifacts/p8-local-fast/6ad14ae2-3544-440f-a147-b0fe59501fd5/manifest.json`.
Its SHA256 is `e5cfa53750aa9bf18c18157567da56661270121107a3f01f89989807598b3871`.

Focused lifecycle Clippy (`--lib --no-deps`) passed with 18 existing lifecycle
warnings and one retained Munarium warning; none targets the new launch guard.
The initial broader `-D warnings` attempt failed on four existing domain
warnings. Both logs are retained under `artifacts/p8-local-clippy/`; this is not
a warning-free workspace claim, and no lint allowance or unrelated refactor was
introduced to make that attempt pass.

The final debug qualification build passed all six MCP unit cases and all three
native-launch regressions before preserving the CLI and integration-test
executables. Source inventories matched before/after the build. Receipt:
`artifacts/p8-local-builds/075f33cf-6670-4caa-b489-496814433d90/build-receipt.json`,
SHA256 `294f8ad27b3d1395d1040985fc305032547b260c9b3d7355aed739ace6326b56`.

| Final artifact | SHA256 |
|---|---|
| CLI executable | `afdd9e0011601c059d82f6f1cc264b59e5a2627f20db36774c701ce5734f4bbd` |
| Unsigned ZIP | `f4045381457ddc84e1c32ff4108622523dba1e6851d779628ed2eb42101064b7` |
| Package result/inventory | `8cd0f1135e63837501b37f9bdbe313383dc347b8d0bc4b9589a33a5190e64956` |
| Fresh-profile distribution smoke receipt | `cda9cc78e22098239aad763f6ca2ea2617822bdfd772a96c92b0c6e15e8c94ae` |

Package inputs are selected explicitly; no private fixtures or local stores are
included. The package receipt is under
`artifacts/p8-local-distribution/3f7a6bec-a374-487a-8d0a-06134e079e78/`.
This build includes the later MCP preflight fix and supersedes the earlier
`4d146515…` executable used to close the maintenance launch finding; that
historical maintenance receipt remains unchanged.
The exact ZIP installed into a new path containing spaces/Unicode, ran help,
version, doctor and missing-profile diagnosis, and uninstalled while preserving
workspace/history/key/vault sentinels. The smoke receipt is in
`%TEMP%/vcp-distribution-qualification-9d210176-8339-4954-b374-33e0d3906cb9/`.
This isolates profiles on the existing host, not an independent clean OS.

The packaged model provisioner verified the existing ten MiniLM files
(91,578,299 bytes, revision `1110a243fdf4706b3f48f1d95db1a4f5529b4d41`)
without downloading assets. The first smoke attempt on the earlier candidate
incorrectly put its data fixture beneath the Git checkout; path policy refused
it. That failure remains at
`artifacts/p8-local-distribution-smoke/756da1b6-2c9a-4b4b-9c91-af264f03fb89/`.
The fixture location was corrected; the path policy was not changed.

The broad matrix retained 36 passing cases, one failed history-fixture
assertion and the explicit machine-handoff gap before a bounded stop. It is
preserved at
`artifacts/p8-local-matrices/e21f5098-a79b-4ebb-8d6a-6d469c91ee7d/`.
The history fixture treated the artifact producer label as an event ID; the
correction instead checks real artifact references, scope, byte counts and the
continued presence of those event IDs after compaction. This preserves the
provenance requirement. The unfinished packaged long-check attempt was stopped
and is not counted as a pass. Surviving test children were explicitly reaped.

The 36 passes include independent Go-age/Node validation of fresh encrypted
snapshots and local cross-backend restore, packaged MCP credential preflight,
four native >120-second check arms, actual CPU embeddings/cache/resources/pause,
publication/query, sixteen generation-kill boundaries, and retained-vector
restore. The matrix's raw incomplete state and interruption receipt are kept;
the broad run is not relabeled as a green campaign.

The focused run recorded eight more passing rows: corrected packaged history,
packaged long-check completion and pause, four retention cases, and native MCP
HTTP 401 handling. All five new packaged regression tests therefore have passing
evidence across the two runs. The history case took 281.21 seconds across both
stores; the long-check cases preserve actual process, verification, accounting
and fresh-inspector assertions.

The focused supervisor enforced its ten-minute cutoff. The final MCP content
conversation printed one passing test result, but the runner did not capture a
complete command exit receipt before termination; it is **not** counted as a
passing matrix row. The final malformed-executable row was not rerun in this
matrix. Its earlier exact unattended both-store result and the final build's
three unit regressions remain separately recorded in the maintenance report.
No additional native campaigns were launched after this cutoff.

The bounded disposition has **44 recorded passing rows and three not-run
rows**: `p8-03-native-mcp-content-conversation`,
`p8-01-malformed-native-executable`, and the owner-skipped
`p8-03-packaged-encrypted-recovery`. This is an incomplete formal campaign,
not a release gate pass. The first two rows still need complete current campaign
receipts; assertions observed without the required command receipt do not waive
that requirement.

The disposition is `artifacts/p8-local-final/result.json`, SHA256
`62db20f190ddd79a27ca0174558099f4e8e76121d258229b57dd495ef7b7a678`.
Focused logs are under
`artifacts/p8-local-supplements/fd3354b5-39b5-4538-98b8-0c9ba1108631/`.
Final source comparison found exactly one changed input between runs: the
history test fixture. All product inputs matched, the focused source inventory
remained stable, and the preserved/compiled/extracted CLI plus the complete ZIP
and extracted inventory retained their exact hashes. The rebuilt test executable
is identified separately in the disposition. No passing rows were inferred from
registration alone or from the failed history assertion.

P8-06 is complete; this PR delivers an explicitly partial P8-01 through P8-04
qualification increment. Those work items and P8-05 remain open. Clean-OS
coverage, independent-machine recovery, owner release acceptance, signing and
publication remain outside this local result. Additional standalone restore
crash campaigns were not run in this bounded continuation; the executed snapshot,
store, generation and local encrypted-restore rows retain their stated scope.
