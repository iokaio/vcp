# P8-06 upstream maintenance rehearsal — 2026-09-22

Status: complete for the bounded P8-06 maintenance rehearsal. The adjacent immutable Codex update was reconstructed and reviewed; its initial native matrix completed only after two recorded Windows dialog interventions. A subsequent production launch fix passed three fresh unit regressions and the exact affected verification case unattended on both stores. All 36 existing patches replayed unchanged, with no conflicts. This rehearsal does not qualify every retained upstream feature, an actual MXC deployment, or the remaining P8 release gates.

## Selection and reconstruction

| Identity | Previous selection | Qualified selection |
|---|---|---|
| Codex revision | `3d3ae4965ab370217e871b3a7f0d15589557ee4b` | `8b78600dc85cc265d7e7e827f6aa903875405287` |
| Git tree | `b54522de663019e332772544ae53054aa3ecb92d` | `58cb6e23d619710a2045e942759246ab54e41a2b` |
| Original inventory SHA-256 | `0731feb5908c07319e74229723db80320517dd80afbb94bc6cec0768100f8259` | `fea602dea49975485f9c168d845de27669d5bc81c0a3063f1197bc77d9dcfc55` |
| Selected result SHA-256 | `a44d30203b4955e4f27cf09fa13093b8bd7e1406c8a8ae738d7d489e1d18fc93` | `54d0aaca87f5f142b4414fb0cacabfa8db4bc4bc8a86092020d9750b53acd986` |
| Selected files | 7,939 | 7,940 |

The candidate is the direct successor of the old pin: [Enable MXC selection through Windows sandbox configuration](https://github.com/openai/codex/commit/8b78600dc85cc265d7e7e827f6aa903875405287). The exact public commit and parent were fetched into a fresh object store. Production reconstruction produced two identical complete inventories and matching trees in 59.255 seconds. These are independent materializations using the same object store and reconstruction implementation, not independent acquisition or implementation.

The complete 72-file upstream delta (+557/-142) was reviewed with old/new byte identities. `codex-rs/protocol/src/sandbox.rs` is the sole added file. All 167 compared upstream dependency/toolchain inputs and 12 rights files are unchanged; the update adds no dependency pin, native/model asset or license text. Existing VCP modification notices, patch hashes and license materialization remain intact. Current attribution and pin references are updated; historical baseline reports and original patch provenance retain their original revision.

## Authority review

The update carries an explicit sandbox type separately from the legacy Windows sandbox level through environment, command, patch, terminal approval and metadata paths. Its upstream managed implementation list restricts Elevated/Unelevated but permits explicit MXC; MXC also defaults to no private desktop. Those are material upstream semantics, not authorization to enable that backend in VCP.

The current VCP configuration uses default retained configuration with empty overrides, disables retained feature flags and telemetry, starts without retained environment discovery and supplies only the VCP tool set. Canonical admission and native effects remain owned by the VCP broker. Static review plus the retained permission tests and connected VCP authority/process/recovery tests below support this bounded adoption. No managed policy, parser, permission assertion or sandbox test was weakened to make the update pass.

The static boundary check passed for 175 packages, 38 groups and 105 seams (7 pure, 17 read-only, 81 effectful). It remains a static inventory. The upstream `mxc_config_routes_command_and_patch_to_the_windows_executor` test returns before assertions outside its Wine test environment; this rehearsal does not count it as executed MXC evidence. Arbitrary child-process filesystem isolation and optional upstream entry points retain their separate qualification limits.

## Native qualification

Native Windows x64 used Rust 1.95.0 and MSVC 14.44.35207, with locked, offline Cargo and two build jobs. The existing Cargo target was reused serially after the P7 gate released it. This is cache reuse, not an independent cold compilation. The separate source graph contained 9,323 files (94,576,856 bytes), SHA-256 `ff823c0bb938510b80f0f88387885938280d1866af03f47eec54e47e9c89994c`; complete source hashes were checked before and after qualification.

The graph captured frozen working P7 bytes before the final commit. A separate comparison binds all code, tests and invoked native harness to commit `46a28c33bafb38723c266505da0d01209a6ea2d6`, whose committed tree is identical to merged P7 `74da9bb23c76297f349d986ee73be61c81126d48`. The receipt discloses eight documentation differences, a line-ending-only Maven fixture and the preexisting unused P8 registry v3. It does not claim whole-tree equality. New P8 package tests in the delivery worktree require their own final package build and results.

| Stage | Passing executions | Explicit ignores | Result |
|---|---:|---:|---|
| retained-config | 3 | 0 | Pass |
| retained-sandbox | 58 | 0 | Pass |
| retained-core | 27 | 0 | Pass |
| retained-patch | 65 | 0 | Pass |
| vcp-controller | 3 | 0 | Pass |
| vcp-policy | 6 | 0 | Pass |
| repository-tools | 61 | 0 | Pass |
| canonical-contracts | 98 | 2 | Pass |
| connected | 144 | 10 | Assertions passed after two recorded modal interventions |
| verification-pause | 1 | 0 | Pass |
| terminal | 68 | 1 | Pass |
| native-cli | 7 | 0 | Pass |
| child-output-loss | 1 | 0 | Pass |

Counts describe observed test executions by stage, not unique tests across stages. The existing matrix separately repeats its required verification-pause case.

The first sandbox invocation used the source filename `manager_tests` as a test filter and executed zero tests. Its required-test guard rejected the stage despite Cargo exiting zero. The failed receipt is retained. A linked continuation ran all 58 sandbox library tests and the remaining stages; successful build/configuration stages were not repeated. No candidate source changed between those runs.

The final connected case, `verification::native_verification_preserves_uncertain_dispatch_intent`, intentionally launches a 28-byte invalid executable. Windows displayed an **Unsupported 16-Bit Application** modal on the native creation thread. The first SQLite arm exceeded 300 seconds, with stationary CPU usage in the final observations; the same dialog appeared on the file-store arm. Each exact-title window owned by test PID 78012 was recorded and closed with `WM_CLOSE`, after which the expected error assertions passed. The two distinct window handles and observations are retained; this is not an unattended matrix pass. The isolated source was not changed to bypass the failure.

This exposed a native-launch gap in the common VCP path. A separate delivery fix wraps synchronous direct, duplex and PTY creation in thread-local `SEM_FAILCRITICALERRORS`, retaining other flags and restoring the previous mode through RAII. It changes no permission, admission, retry or accounting rule. No claim that the upstream revision itself caused this environment-dependent dialog is made.

Fresh closure evidence is separate from the intervened matrix. The three launch unit regressions passed unattended in 0.08 seconds: supervised malformed-image rejection, nested restoration, and restoration on success/error/unwind with an unaffected sibling thread. The exact `verification::native_verification_preserves_uncertain_dispatch_intent` case then passed unattended in 1.46 seconds, executing its SQLite and file-store arms with zero dialog interventions. The selected command, including compilation, took 105.848 seconds. Its source identity remained stable. The focused runner records one passing row and 46 unselected rows, so its full-campaign status remains `incomplete` with runner exit 1; the selected test itself exited zero and was positively observed. This closes the maintenance finding, not the whole P8 campaign.

The first launch-unit run is also retained: one test passed and two failed because the fixtures assumed native error 193 instead of observed `ERROR_EXE_MACHINE_TYPE_MISMATCH` (216), and equated a sibling thread's mode (0) with the process mode (3). The corrected tests require the exact observed native error and compare an already-running sibling against its own baseline. The production guard did not change after that first run. These fixture corrections do not erase the failed receipt or the original two manual interventions.

Existing retained and qualification warnings remain recorded in the logs. Explicit ignored helper/environment-dependent tests are reported separately; no ignored row is counted as a direct pass. Tests use synthetic providers and native local processes, with no paid provider calls.

## Evidence and disposition

| Evidence | SHA-256 |
|---|---|
| Initial native receipt (rejected zero-test selector) | `8e0e3058f1dd5a2a1f4493ee7a9b360b2d1862d053d1581b24f0f7ae3f4d4dc7` |
| Continuation receipt (intervention limitation applies) | `ecb550dafc7541dba420e0081fb28a0591deadf08e60bf31e524111ceedc7de5` |
| Nested full delegation receipt | `afd453b3d561a7a98657d41563fc4583f0b08ef9aeb144be799741d748a896a3` |
| Modal disposition | `c9ef126679b9670a4b6385fae4e8dd60c686626a080cc52556be4cfd51fd58f8` |
| Original rehearsal CLI binary | `676ac3da0dc2383d137ec2c5ecc246febcd4bbfa61574470e3a7192df3f96554` |
| Provisional delivery promotion | `16fd8dbae195ea07f4d1bbd8f7a96d6c5c5a09e72a4f4c1cfe87090afe72895d` |
| First launch-unit log (one pass, two fixture failures) | `596a17ef097163dfd904bec5736546844d445b509d4b5bf897f5b927ac7f1ecb` |
| Fresh launch-unit log (three passes) | `5689d919ac4ee8954411a4238693cef3d255e7c394e25c1186baab46e0cd1b37` |
| Fresh delivery build receipt | `bdecdd47e9da90bb371667f928f63ceee570b59bd77faad866cf1fc43f632e60` |
| Fresh delivery CLI binary | `4d146515c5edf6987cb0ed325958301fe88100b884edbf224fc0d33891e6d1aa` |
| Exact verification disposition | `74a6d02991baac9790c3484e366f95a433b70de3ca8ba30b5797930dd5beba02` |
| Exact verification matrix receipt | `7063b8880a82683c9e631fe634af455851557ec5f33a59662dc58c11ab33b360` |

The preserved original rehearsal CLI contains 250,504,192 bytes. The earlier build-only binary had SHA-256 `a5b885c1ce08718d6ba43fc596d59c3e0f9d972d038852cdce79e8f9cdead226`; the final rehearsal CLI test graph produced the separately recorded original binary above. The fresh delivery CLI contains 250,485,248 bytes and includes the production launch fix. Its binary and receipt hashes were independently checked. All 638 input paths shared by the build and exact-verification inventories have identical hashes; their overall inventory digests differ because the scopes differ.

Measured effort: reconstruction/materialization 59.255 seconds; initial CLI build 453.614 seconds; initial native attempt 899.523 seconds; continuation 4118.207 seconds. The two original native attempts total 83.629 minutes, including compilation, tests and the recorded modal wait/intervention. Static review and time waiting for the P7 Cargo checkpoint are outside these native durations. Conflicts: zero; patch changes: zero. The additional maintenance work is the separately reviewed VCP native-launch fix, the retained failed fixture run, and the fresh unattended checks above; those follow-up runs are not included in the original 83.629-minute total.

Private reconstruction, per-file review, source/checkpoint bindings, stage logs, binary identities and promotion receipts are retained under `artifacts/p806-maintenance-rehearsal/`. The failed launch-unit log is under `artifacts/p8-local-builds/a74106f2-e162-4eaa-8ffb-87d7e3c97ba2/`; fresh unit/build evidence is under `artifacts/p8-local-builds/4e85c371-ec4d-4b37-b696-cab363dceb81/`, and exact verification evidence is under `artifacts/p8-local-launch-exact/9c10b146-9d56-4948-ac6b-ae9efa0f10e0/`. Current reproducibility inputs are the committed [selection](../../src/third_party/components/codex-selection.json), [result inventory](../../src/third_party/components/codex-files.json), [upstream manifest](../../src/third_party/upstreams.toml) and [unchanged patch series](../../src/third_party/patches/codex/README.md). The reviewed source and four identity manifests were promoted provisionally only into the isolated P8 delivery worktree, with the then-outstanding launch gate explicit. The separate fresh checks now close that gate without changing the historical promotion receipt. Broader package, independent-environment and release acceptance remain governed by the [P8 qualification record](p8-qualification-readiness.md).
