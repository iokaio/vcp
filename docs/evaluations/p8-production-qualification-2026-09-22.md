# P8-01/P8-04 production qualification — 2026-09-22

Status: bounded production evidence recorded; P8-01–05 acceptance remains open.
This increment freezes the intended optimized Windows artifact without
qualification features and performs selected artifact-specific checks. It preserves the previous
[history/security campaign](p8-history-security-followup-2026-09-22.md) and its
failed attempts; those debug-package receipts are not relabelled as production
results. Machine handoff remains owner-skipped. Physical full-volume exhaustion
remains an explicit recovery gap.

## Artifact and build contract

The completed build uses the committed Rust 1.95.0 toolchain and release profile, locked
offline dependencies, the Windows AMD64 MSVC target, and the committed static
CRT configuration. `scripts/build-production.ps1` selects only `vcp-cli`'s `vcp`
binary, passes `--no-default-features`, and checks Cargo's artifact receipt for
optimization level 3 and an empty CLI feature list. Qualification endpoints and
fixture executables are not enabled. Build inputs, upstream inventory checks,
compiler output, executable and sidecar symbols are retained separately from the
unsigned distribution ZIP. No signing or publication is claimed.

The completed build starts from source commit
`4a43fa05d5b522bc6f188e1f2e74fe146ba2123b` with the original build script untracked.
Its before/after content identity is
`16f10bef68c7c2ae72e2ee94438ee758b34f30af318aa3122b7cbf5e7535d41b`.
The first build took 2,860.933 seconds. Its original receipt is preserved unchanged;
the later script adds stronger environment/configuration and dependency checks.
A second build solely for that improved provenance was deliberately stopped
after a native dependency invalidated the cache. It is retained as cancelled,
not counted as a successful build.

| Frozen input | Identity |
| --- | --- |
| Build directory | `artifacts/p8-production-build/7a1dd543-78a5-4cf1-852a-3dd28792b257` |
| Original build receipt SHA-256 | `3265a0645e9f44125656f97f3601cee08779986050ce876b2aa390d82fe96bf2` |
| Distribution directory | `artifacts/p8-production-package/eb9212c5-a85c-411c-bb72-b887f81d52bc` |
| `vcp-windows-unsigned.zip` SHA-256 | `5bc5eddedf8a7510661ea6b22efca4c668fd9f684300dac3c5d81afefa6d99d9` |
| `vcp.exe` SHA-256 | `e5f09a9005f54670197427e707fc6fa30e439d8ce20d25f6d7cf90a8abbbee62` |

The supplemental audit in the build directory verifies every retained VCP Cargo
artifact has an empty feature list and optimization level 3, unchanged product
inputs and matching imported-source inventory. The changed build recipe is
explicitly excluded from that unchanged-product assertion. Current native-tool
and Cargo-configuration hashes are supplemental observations, not retroactive
proof of the original unrecorded environment. The cancelled attempt is
`1c686c3a-d89e-4214-9548-8e523513b2ab`.

The package retains its original build receipt and inventory of 53 payload files
plus `manifest.json`; the executable is 187,250,176 bytes. The payload inventory
SHA-256 is `e23c7024d4194fa0d26a770b33fc98823ec5b931971f9efcb74ce495ec811a5c`.
Package `result.json` SHA-256 is
`9041aaed047794bd16aa819d7f10907db955ccbc82231d8cf1f13ba84189cd16`. A
native PE import audit (`pe-dependents.txt`) lists Windows system DLLs and no
separate MSVC runtime DLL. This supports the static-CRT build observation; it
does not establish compatibility with an untested Windows version.

## Bounded startup protocol

The retained MCP history fixtures supply two observed sizes on each backend.
Their content differs as well as their size; this is not a controlled synthetic
scaling experiment. Original fixture receipts and their logs identify each
source. Inventory every source data/workspace file, copy the canonical data for
each invocation, retarget only the copied local descriptor, and verify the
original inventories afterward. No model requests or task resumption occur.

Freeze the fixture identities, independent commit/event/record counts and exact
package digest before measuring. Run three fresh processes for each of
`inspect --view costs --limit 1` and `history list --limit 1`. Record wall time,
CPU time, sampled peak working set, exit status and structured results. Each
command has a 120-second correctness ceiling; stop further work for that
backend after a timeout. The filesystem cache is not flushed. Three samples
support descriptive ranges and medians, not a stable p95 or a minimum-hardware
claim. These bounds are not P8-05 performance acceptance thresholds.

All 24 measured invocations passed their structured result/scope/watermark
checks; original data and workspace inventories were unchanged. Before/after
canonical counts were unchanged in every sample. The host was Windows 11 Pro
build 26200, AMD Ryzen Threadripper PRO 5975WX (32 cores/64 logical processors),
137,295,024,128 bytes of RAM and NTFS storage. Compilation and other qualification
workloads were idle during timing. This is not minimum-hardware qualification.

| Store | Commits / events / records | Inspect median, range (s) | History median, range (s) | Maximum sampled working set (MiB) |
| --- | --- | --- | --- | --- |
| SQLite | 512 / 486 / 932 | 15.197, 15.188–15.332 | 30.341, 28.445–30.533 | 80.74 |
| Files | 512 / 486 / 932 | 15.249, 15.159–15.276 | 28.429, 27.937–30.627 | 73.67 |
| SQLite | 785 / 750 / 1599 | 36.176, 35.734–36.734 | 70.563, 70.274–70.742 | 112.88 |
| Files | 785 / 750 / 1599 | 36.044, 35.224–37.700 | 70.112, 69.917–70.296 | 107.99 |

Production optimization leaves substantial startup cost. The previously
[identified replay/validation and repeated history-open paths](p8-history-security-followup-2026-09-22.md)
remain relevant investigation targets; this measurement does not attribute time
to individual functions or establish a controlled debug/release speedup. No
performance implementation or acceptable interactive-startup threshold is claimed.

Receipt: `artifacts/p8-production-startup/d4421312-d327-4fdd-ba4b-646fb671d546/result.json`,
SHA-256 `5da329a887900990444a9b5789e509fe7a22e86baf461a56b5e9a07a6fc250ff`.
`predeclared.json` binds the four retained campaign receipts and independent
counters. `executed-runner.ps1` preserves the measured supervisor exactly.
`summary.json` derives the table and byte totals. A metadata-only runner issue
left its original `bytes` fields null: `Measure-Object` did not read the ordered
dictionary keys. The retained hashed inventories supply exact totals of
12,866,531 / 14,342,200 bytes for SQLite and 5,126,668 / 8,310,891 for Files
(smaller/larger cohort). The future runner now sums explicit keys and explicitly
rejects any changed canonical count. Independent audits found no count differences
in all 24 retained samples. The original timing receipt is unchanged; these
metadata/validation corrections do not require repeating its timed invocations.

## Targeted installation and recovery

The production distribution runner passed 16 commands and 32 assertions,
including exact-ZIP fresh-profile startup on Unicode/spaces paths, prior-package
installation, incompatible declared-state refusal, interruption before activation,
retry, production read/write of real storage preferences, rollback while the new
payload is locked, locked uninstall refusal and successful uninstall preserving
state and workspace/key/vault sentinels. This is a clean installation and fresh
profile on the current host, not a clean Windows OS. No cross-format migration
or fresh model acquisition is inferred.

Distribution receipt under system TEMP:
`vcp-production-distribution/a9bf4451-4cdf-4f70-9177-873fd3ea5c54/result.json`,
SHA-256 `68b9c803a56d211794a6033f15918a15d8eddbbfc7b039c7213de4b4d8ffabf2`.
The failed preceding run `d4e3a736-e75a-4d73-bd28-6e367689ed17` is retained:
PowerShell resolved the runner's `Cli` helper as its `Clear-Item` alias. Renaming
the helper fixed the harness; it required no product or artifact change.

Configured-root recovery passed 70 bounded steps across both stores. Production
rejects the qualification-only profile setting, wrong/missing keys, tampered or
truncated ciphertext and unsafe staging. Authenticated local restore preserves
source bytes, root/child paused state and untrusted authority without replay or
fabricated Git state. Only the disposable restored workspace receives explicit
Git initialization, rebind and ordinary revision-checked trust before fresh
manual backup; trust remains a prerequisite for source capture.

Each fresh production snapshot passed pinned independent Go age decryption and
Node Ed25519/domain/inventory verification: 66 payloads, three source files, two
task records and one ledger record per backend. Prior debug executable
`0087829f97e4e13b3e36198449de5af47b1f346e6f54d419aa33f0bb83f552b3`
then reads those same tasks/ledger, followed by fresh production reads; the
descriptor and ciphertext remain unchanged. This proves same-format canonical
read compatibility, not downgrade writes or migration. Final ciphertext scans
are not concurrent interrupted-publication observations or the full security matrix.

Recovery receipt under system TEMP:
`vcp-production-recovery-806f0451-8fd1-4eb5-99b0-bf12acdb1b7c/result.json`,
SHA-256 `6ea6ce100e9017379b25ee26ddc3d7527f322675ab1aeada7c2820ed64092e7b`.
Earlier failed roots remain: `737e2e27-8327-415d-84bf-c081075ae3ae` (default-path
issue below), `54e8c047-e5d1-4694-add9-458dd631ff72` (sandbox could not read the
retained synthetic key), and `3add1e4d-0642-4689-99a7-b028f18e6876` (correct
refusal to capture an untrusted restored workspace).
The latter was addressed with explicit trust through the existing contract.

**New production gap:** fresh restore to a nonexistent destination, without known
or explicitly declared sync roots, rejects `workspace access denied` before key
authentication. The CLI's optional `--sync-root` permits that invocation, but
[restore](../../src/crates/vcp-cli/src/restore.rs) removes the data root from the
trust exclusions and can pass an empty list to the private-path guard, which
[rejects empty exclusions](../../src/crates/vcp-store/src/private_paths.rs).
The passing run supplies a disjoint existing `--sync-root`; it does not close this
default-path gap. No guard was weakened and no product code was changed.

## Interactive execution gate

The prepared plan binds the exact archive, all extracted payloads, profile/catalog,
synthetic files and native ConPTY/export helpers. It declares a single read-only
task, two requests maximum, a $16 root allocation, 90-second overall bound and
60-second provider timeout. Same-process/task pause and resume acknowledgements
and terminal exit each have a five-second ceiling; the paused observation is one
second. These are lifecycle checks, not coding quality scores or wire-level
dispatch measurements. Exact captured credential occurrences fail before redaction.

Final plan under system TEMP:
`vcp-production-interactive-34819a08-ccff-4572-8fc7-7c2a88af7cc8/plan.json`,
SHA-256 `0260a759d5cf3157c856fb5aed5ab8775541341ca1943ebc15b9e2766155f4dc`.
An earlier zero-call preparation (`90bd3034` prefix) was superseded before launch
after the profile's required explicit path scope was corrected. Plan authority
and empty automatic effects remain unchanged.

**Not run:** automatic approval review rejected launching the paid provider probe
because it did not find explicit authorization for the OpenRouter destination,
synthetic payload, existing credential use and $16 allocation. The process did
not start; no `run-once.json`, provider call or campaign reservation was created.
Explicit approval was requested for this probe and the separately proposed owner
tasks. The existing ledger remains $1.278161 settled plus $34.127269 reserved
against $100, leaving $64.594570. Prior unknown charges remain reserved.

## Acceptance boundary

P8-05 still requires owner-approved held-out task definitions and numeric
quality/cost/latency limits before final scoring, followed by factual human
review. Previously exercised P7 tasks remain useful regression inputs but are
not renamed as held-out acceptance. The existing
[owner packet](p8-owner-acceptance-package-2026-09-22.md) remains NOT APPROVED.

The existing campaign ledger retains unknown prior charges. Artifact-only
startup, installation and recovery checks make no paid provider calls. Any
interactive provider check must reserve its declared cap before launch and
count failures and uncertain charges; it cannot supply human owner approval.

A concrete [portable owner fixture proposal](../../src/evals/release/p8-owner-v3/README.md)
is now available, with [manifest](../../src/evals/release/p8-owner-v3/manifest.json)
SHA-256 `6a27284e55f440ffbc8580562b415f8cab1157e54b569dde88fbe07ec5c8969b`.
Its 44 content files are frozen separately from the manifest. Three new synthetic
analysis/review/generation scenarios are proposed once per store (six attempts),
with hidden truth outside model-visible roots and a portable bootstrap for the
required Git states. The reference generation passes 37/37 hidden assertions;
the incomplete starting implementation passes only 9/37, and the visible baseline
passes 3/3. Both seeded review defects were reproduced. These are fixture/oracle
self-checks, not live task scores. Static review added a minimal U03 Node test
declaration and an argument-restricted launcher for current-parent verification
before any paid attempt. The prior 42-file packet and its manifest
`86b7b39cba7ee000d3913f50245f8f7f6c0418fc3a5b1eac26a64f053b0282cb`
remain retained as the superseded proposal.
The final license-metadata revision appends Apache-2.0 SPDX comments to existing
first lines, preserving citation line numbers, prompts, gates and runtime
behavior. Its preceding 44-file Node-verification manifest
`c39ab2d89dff5bb08b4cf8f3e8365a56420ce169e54c579dd5b496412256ba0b`
is also preserved intact; neither revision was submitted to a provider.

Proposed owner gates are zero false/unsupported findings or forbidden changes,
all specified feature/preservation checks, and 3/3 human rubric criteria per
scenario. Proposed ceilings are $8 per root task, $48 aggregate, 900 seconds per
task, 360 seconds per response, 16 requests, 16,384 output tokens per request,
and zero retries, repeats or unplanned interventions. They await owner approval.
The provider, current qualified catalog/profile, integrated routing/delegation
setup and exact installed artifact must also be bound before execution.

The available Qwen snapshot has `byte_ceiling_qualified=false` and input capacity
983,616. Its recorded conservative per-request reservation is $6.492808:
$1.967232 ordinary input + $1.967232 cache read + $2.459040 cache write +
$0.098304 output + $0.001 request. A $1 allocation cannot admit that request.
The proposed limits preserve that qualification. One separately capped $16
interactive probe permits two reservations across pause/resume; together with
the six proposed owner slots, $64 fits the observed $64.594570 campaign headroom.
This is allocation evidence from the existing qualified snapshot, not a current
provider price quotation or approval to reuse expired qualification. No owner
slot is launched by preparing these fixtures.

The final proposal was materialized into six private roots, with source/Git
states and generated profiles frozen in
`vcp-p805-owner-prepared-bb396ee8-8317-4e6b-a8fd-2ddf51eadd2d` under system TEMP.
Preparation SHA-256 is
`e863cd9ca6d2cfea6ab6b7f0d0b565fed775337a91c104d082092b3fd63237c8`;
the execution binding SHA-256 is
`6aff89eb5c2e603a0858c4ab23d961b92cdd7963666685cb3293a400aa7e2ce1`.
All six exact production profiles pass credential-free startup preflight, with
backend preference configuration but no accepted canonical task, provider call
or reservation. The first offline preparation (`22186023` prefix) is retained:
its empty read-only path scope was rejected by the profile contract. Explicit
fixture paths fix that setup without changing Plan authority or granting writes.

The pagination launcher was compiled with the pinned Node runtime and tested:
its sole declared invocation passes the three visible tests; alternate arguments
are rejected. Build receipt:
`artifacts/p805-page-launcher-d93e2988-573e-4e75-bf57-63fe447542cf/build-receipt.json`.
The final fixture self-check again passes 37/37 reference assertions versus 9/37
on the incomplete baseline; both review defects are reproduced. Hidden grading
truth remains outside the six model-visible roots. Natural-language finding and
human usefulness judgments are not replaced by keyword matching.

The exact offline build/binding capsules and integration review are retained
under `artifacts/production-draft/`. These six direct coding tasks still omit
populated cross-workspace memory isolation, visible child review and the complete
integrated U01–U09 schedule. Their prepared state does not close those gates.
No owner quality score, factual human approval or publication authorization exists.

## Remaining conditions

Validation: all 17 `scripts/test.ps1 -Suite fast` groups passed in 39.775 seconds,
receipt `artifacts/p8-production-fast/99a0f32e-182a-4ea1-a1df-e0e59d3c8b9c/manifest.json`.
PowerShell AST, Node syntax and Python AST checks passed for the new runners;
fixture self-checks, launcher positive/negative controls and six credential-free
profile controls passed as described above. The stronger build recipe's second
compilation was deliberately cancelled; its post-build rejection path and the
completed first build's supplemental feature/input audit are separately retained.

P8-01–05 remain in progress: paid interactive execution and scored owner tasks
await explicit authorization; the discovered default restore-path issue remains
unfixed; startup remains slow with retained history. Clean-OS/fresh model setup,
the full integrated owner schedule and final human acceptance remain separate
requirements. Physical full-volume exhaustion remains an explicit recovery gap.
Machine handoff remains skipped by owner direction, not recorded as passing.
The previous full matrix and failed attempts are preserved; no second full matrix
was run and no release was signed, published or deployed.
