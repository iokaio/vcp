# P8-03 local history and security follow-up — 2026-09-22

This bounded increment follows the [remaining recovery campaign](p8-local-recovery-followup-2026-09-22.md).
It preserves the previous [current-machine receipts](p8-local-qualification-2026-09-22.md)
and [release scorecard](p8-release-scorecard-2026-09-22.md). Machine handoff
remains owner-skipped. This is unsigned debug qualification, not production
artifact or release acceptance.

The selected local P8-03 increment passes: private staging exclusion, packaged
sensitive surfaces, independent crypto and packaged MCP history through
compaction, fresh owners and purge. Both stores pass against the exact candidate
identified below where packaged execution applies. The startup-classifier defect
found during qualification is corrected with positive and negative regressions.

## Candidate and schedule

The initial extracted candidate, used for the historical sensitive-surface receipt, is
`artifacts/p80203-distribution/4bc90125-0495-416f-8bc1-31525b4c0622/extracted`.
CLI SHA-256: `4343607c43def35e244c2f2d89f8b297bb7c1a06e387dcab33c5a00e5a541874`.
ZIP SHA-256: `a84be61ec867293c514a89e65b3ed254c1df89560b8859334ea0c17ada096979`.
The package inventory and extracted CLI bytes were verified. The build uses
Rust 1.95.0/MSVC with `qualification` enabled; the fixture checks the packaged
executable digest against its compiled CLI. Subsequent test-only corrections
have their own source inventories and do not relabel earlier receipts.

The candidate's bounded build result is
`artifacts/p80203-recovery/build-6263d11c-9cfb-44a2-b430-16331739c731/result.json`,
SHA-256 `a175a7d809dd2b8dafa491e8e0ee1de0515df4a87e8c48bf11d7aac07cd5ab6a`.
The packaged `build-receipt.json` has SHA-256
`43e2531cd11a42acf2ac75bce831817646714ed12081c7549219d568a73c88ba`.
Comparing the build's source inventory `97f095829cea532028a5a7bf248b04cde6505e7a010cd53be64f179f95daf737`
with the selected sensitive tests' `e13e588942e462c3406e6116921596590458c8ccd29f63b123bcd8ca5fb7beec`
shows only two changed test files: `packaged_sensitive_surfaces.rs` and
`snapshot_sensitive_surfaces.rs`. Product bytes did not change, and the packaged
sensitive fixture's compiled-versus-packaged CLI digest assertion passed. These
identities remain historical after the startup-classifier product correction.

The post-correction candidate is
`artifacts/p80203-distribution/84ae1108-fd49-421e-baa0-839b1964ee16/extracted`.
CLI SHA-256: `0087829f97e4e13b3e36198449de5af47b1f346e6f54d419aa33f0bb83f552b3`.
ZIP SHA-256: `596dfd5e8d532cfb5dcc8b81afbd0531c74b76434a96be809fba8f5aaaec5376`.
The package `result.json` has SHA-256
`c73c48a0883816aec3b645951966126c1a40d040644ee8ebc86a432194b6cca9`,
and its packaged `build-receipt.json` has SHA-256
`462d790bd2cdfe0641d6ff9c8983133b733b3a18c1d0183d8952609153bdaef5`.
All 53 extracted inventory entries and the archive were rehashed and matched.

The corresponding build result is
`artifacts/p80203-recovery/build-9016af96-004a-4938-ac43-b62f8b27252b/result.json`,
SHA-256 `4ac7735361e495b087bf2146a48bb219816e51c1705dfa25d99072459c1d96d0`.
The bounded no-run build passed in 33.866 wall seconds against source content
`0bb7251e3e4246d43af2be1f1886059a70c39bbe3d5a584bd3ccf8a60373a527`;
its five referenced files match. Final sensitive surfaces, independent crypto
and MCP history pass against this candidate. Earlier sensitive/cloud
restore receipts keep their original candidate identities.

Each selected Cargo invocation has an absolute 600-second compilation and test
deadline, followed by bounded process-tree termination/output collection. Each
receipt records the exact command, test name, exit status, source identity
before/after and hashes for both logs. Package assembly has a 180-second bound.
Only these missing cases and the existing independent crypto case are selected;
the prior full matrix is not repeated. Repeating the crypto case binds its
independent verifier to this changed executable.

Following the observed MCP failure, its unchanged semantic schedule is split
into separate Files and SQLite wrapper tests. Each direct compiled-test
invocation has a predeclared 1,200-second outer deadline, with per-CLI kill/reap
bounds retained. The earlier diagnostic observed about 22 seconds per inspector
startup and 44 seconds per history startup; repeated fresh-process inspection
made the original two-store 600-second schedule insufficient for a complete
run. This is a newly declared bounded schedule, not an extension of a running
deadline or a relabeling of the earlier failed attempts. No full matrix rerun
is planned. The first split wrappers ran independently on separate fixture roots,
without Cargo, using compiled harness SHA-256
`33a592fab9d8febfe152e2dc3a4b067c71f959d59810a3128f1f6afe3f71b116`.
Their exact command receipts name the store-specific selectors. Sensitive and
crypto tests run serially with their 600-second bounds. These overlapping
qualification runs make no latency-benchmark claim.

Both first split attempts hit the inner 90-second CLI bound during the first
fresh owner's final verification sequence, after provider request 106. Neither
hit its 1,200-second outer bound. The second split schedule declared a 180-second
MCP CLI bound before starting; its outer bound remained 1,200 seconds. Four temporary
diagnostic-only Store/spool opens per case were removed after they identified
the startup defect; no semantic assertion or required fresh CLI reopen was
removed. This responds to measured fixture execution costs and does not change
production timeouts or establish/relax P8-05 quality, cost or latency thresholds.
The changed harness build passes:
`artifacts/p80203-recovery/build-a1c9c0e2-b11b-4a7d-8c68-6da74728b968/result.json`,
SHA-256 `56d849319908bbf31cf913e994f9543f3fbb1c3ede2565fc4734f8c1f0abe87a`,
19.306 wall seconds, source content SHA-256
`fba77faae80fa379c918db27bae97505c5038352bffd50b9ed1cf4fcd6b4f969`.
The result and five referenced files were rehashed and matched. Comparison with
the `9016af96` build inventory shows only `packaged_mcp_history.rs` changed.
Both the compiled CLI and extracted package still hash to the exact
`0087829f97e4e13b3e36198449de5af47b1f346e6f54d419aa33f0bb83f552b3`
product executable. The new harness SHA-256 is
`eda6de4bded6c6f493ebb726ce1e54052e54d67547b7da899bd12c2b6bbb24a9`.
The SQLite `mcp_sqlite-a1323d9e-4f82-47c1-9eae-8b314a665500` and Files
`mcp_files-8cc78b59-3ba4-4d98-ace8-1d469c167c55` attempts use that harness under
the newly declared bounds. Both failed the inner 180-second bound in their
second fresh owner (`revoked=true`), with the outer 1,200-second bound unexpired.
The first fresh owner completed in 112.247 seconds on SQLite and 113.091 seconds
on Files. At that stage the full MCP acceptance schedule had no passing receipt.

The final correctness schedule predeclared 600 seconds per MCP CLI invocation
and 2,400 seconds for each store-specific test. Every semantic step and assertion
is retained. The measured 90-second pre-admission interval at the third owner,
continuing canonical events and increasing fresh-process inspection cost on the
accumulated history justify a finite schedule covering the complete workload.
This does not raise production timeouts, establish a latency target, or qualify
P8-05 acceptance. Debug startup/history latency remains an observed limitation.
No earlier deadline or failed outcome is rewritten. The new bounded build
passes in 24.749 seconds:
`artifacts/p80203-recovery/build-cddea95e-47af-45f9-b072-ef882b962170/result.json`,
SHA-256 `ec19490c98945044d31d1b73a72b7d79a0e0bb80f60563cfe58036e2494f1194`,
source content `dcf2ff1f46148564b7aa35119464ebffe76d0a19b7b2869d58517a4debd03fba`.
All five referenced files match. Comparing its inventory with `a1c9c0e2`
again shows only `packaged_mcp_history.rs` changed, and compiled/package CLI
digests both remain the exact `0087829f` candidate. The new harness SHA-256 is
`b72b59acefbdba8d7fbfca2820bf27d97bd049db0547ff1e0c5c25b5dec4d77c`.
SQLite `mcp_sqlite-e9bd65b6-8f40-4e50-87a2-46210f9e1b46` and Files
`mcp_files-931fe51c-e0d6-44e3-8bd1-732d5e596b1b` use this harness with the
600/2,400-second schedule. Both complete successfully, as recorded below.

## Evidence

The
[sensitive-surface map](p8-sensitive-surface-map-2026-09-22.md) defines the exact
typed-secret versus ordinary-text boundary.

| Selected case | Verified scope | Wall / test seconds | Receipt |
|---|---|---|---|
| Private snapshot staging | Files and SQLite: inspect the actual private neutral `.archive`, including decoded JSON byte-array payloads; exclude exact disposable age identity/writer seed material while retaining ordinary scoped source text; verify the accepted digest and cancellation cleanup. | 6.194 / 0.14 | `artifacts/p80203-recovery/sensitive_stage-8b03c29d-5662-43e0-97a2-f07e206c3fbc/result.json` |
| Packaged sensitive surfaces | Post-correction `0087829f` candidate, Files and SQLite: exclude disposable provider and generated key material from provider bodies, captured artifacts, canonical state, JSONL/diagnostics, all inspector pages and real ConPTY output; prove transport authentication remains present and the native child environment excludes both secret variables. Ordinary token-looking source text remains visible. | 259.350 / 254.22 | `artifacts/p80203-recovery/sensitive-e8863ef3-5abf-45f4-9f92-d93be17ca64c/result.json` |
| Independent packaged crypto | Post-correction candidate, both stores: independent age decryption plus signature, enrollment, scoped inventory and all payload digest checks; key/provider exclusion, wrong-key/tamper refusal, preserved old key after rotation, and exact opposite-backend local restore without task replay. | 187.515 / 182.58 | `artifacts/p80203-recovery/crypto-056f6858-e4dd-4a43-a40a-6f2bb847f9ba/result.json` |
| Packaged MCP history — SQLite | Exact candidate: discovery/resource/prompt bytes and provenance survive compaction and fresh CLI inspection; fresh owners reject stale cache/identity and revoked resource access; filtered purge reports gaps without replay. | 1740.261 / 1738.60 | `artifacts/p80203-recovery/mcp_sqlite-e9bd65b6-8f40-4e50-87a2-46210f9e1b46/result.json` |
| Packaged MCP history — Files | The same complete schedule on Files, with independent fixture root, request observations and final purge inspections. | 1733.515 / 1732.41 | `artifacts/p80203-recovery/mcp_files-931fe51c-e0d6-44e3-8bd1-732d5e596b1b/result.json` |
| Acknowledged partial capture startup | Both stores and stdout/stderr/child-transcript: a fresh owner admits a new root and output after exact canonically acknowledged deliberate partial capture, preserving old bytes, receipts and event prefixes. | 35.263 / 0.43 | `artifacts/p80203-recovery/partial_ack-daaf7ccd-6773-4130-9ffb-9e1aa9c5861f/result.json` |
| Startup negative guards | Both stores × unacknowledged, physical Pending, mismatched, CaptureFailure and provider captures: admission remains fenced and physical bytes remain unchanged. Mismatched canonical insertion is rejected by Store before startup. | 30.814 / 1.50 | `artifacts/p80203-recovery/partial_guard-9a8ebec9-66a5-466a-b6bd-e4aad2052c82/result.json` |
| Existing capture-capacity regression | Files backend: actual request/response capture limits still fence transport, retain the exact acknowledged prefix, pause the task and preserve unknown response liability across reopen. | 5.758 / 0.85 | `artifacts/p80203-recovery/capture_guard-de102ab4-aefa-4503-a76b-cfb180fc2fbd/result.json` |

The selected staging result has SHA-256
`b21dae5d02cac498d701e4a000b37426c02a4e8079b12e7ad2e7fd99ff65710b`.
Its exact selector is
`private_snapshot_staging_excludes_key_material_but_retains_scoped_ordinary_text`.
It reports exit zero, no timeout and stable source during its 600-second bound.
The source content SHA-256 is
`e13e588942e462c3406e6116921596590458c8ccd29f63b123bcd8ca5fb7beec`,
with dirty source based on `4551fa0cc735948819efbf246101f8e2ff0450bd`.
The result and all five referenced command/log/source files were rehashed and
matched. This staging component test makes no packaged executable assertion.

The selected packaged sensitive result has SHA-256
`dfac286c11c59a085819dfc8520eff43df5ad5532ba2a6e3ec1f2fe0f4b26d9c`.
Its exact selector is
`packaged_sensitive_surfaces::packaged_typed_secrets_stay_out_of_capture_environment_and_presentations`.
It reports exit zero, no timeout and stable source under the 600-second bound,
with final source content hash `0bb7251e3e4246d43af2be1f1886059a70c39bbe3d5a584bd3ccf8a60373a527`.
Its result and all five referenced files were rehashed and matched.

The earlier passing sensitive receipt is preserved:
`artifacts/p80203-recovery/sensitive-ecf570da-0581-4aab-a983-18e42ef563fc/result.json`,
SHA-256 `4b1c438fd0eaa3cc40219b7538835cac8fc465c286ffadf3a822653d8d25f42b`,
source `e13e588942e462c3406e6116921596590458c8ccd29f63b123bcd8ca5fb7beec`,
264.781 wall / 244.59 test seconds, against the original `4343607c` candidate.
It is not relabeled as a run of the post-correction executable.

The logs confirm 78 retained artifact payloads scanned on each backend. Ten
inspector views were fully traversed per backend: the chain view had two pages,
and each other view had one. Generated age identity and exact writer seed
checks include rescanning original key-create stdout/stderr after seed discovery.
Native terminal artifact inspection uses actual ConPTY output and requires the
positive ordinary marker; it is not interactive pause/resume qualification.
Artifact metadata records `authentication_headers` and `recovery_material`
omissions, with no invented retained offsets. Inspection and diagnostics cause
no extra provider dispatch.

The final MCP SQLite and Files results have SHA-256 respectively
`cb3c83f280b2bd9b9533633194a86d72e8f048c9614d454e5aa684aefb153f51`
and `697402f02adb42eb27c55081a4d02572afc07b4daf53b42497bca36fd7bd005b`.
Both report exit zero, stable source, no outer timeout and source content hash
`dcf2ff1f46148564b7aa35119464ebffe76d0a19b7b2869d58517a4debd03fba`.
Both results and all ten referenced command/log/source files were rehashed and
matched. The exact selectors are
`packaged_mcp_history::packaged_sqlite_mcp_content_identity_survives_compaction_reopen_and_purge`
and `packaged_mcp_history::packaged_files_mcp_content_identity_survives_compaction_reopen_and_purge`.

Each case compares full discovery, resource and prompt artifact bytes, descriptor
digests, configured server identity, protocol/admission identity, registration
link, source/context provenance and original event links before and after
compaction through fresh CLI inspectors. Ordinary Unicode and JSON-looking text
remains byte-exact; hostile instructions remain external data rather than
system/developer instructions. A fresh owner cannot reuse the old live cache or
connection identity. Another fresh owner with a changed resource allowlist
rejects the old resource as undiscovered/not allowed. Both owners still finish
their ordinary edit/verification tasks. Filtered purge then yields explicit
pruned gaps for the three retained observations without exposing their content.
Independent MCP method markers and provider counters remain unchanged by
inspection, compaction and purge; stale access does not replay content reads.
This is stdio qualification, not packaged authenticated HTTP qualification.

The independent crypto result has SHA-256
`eb5b06142fd2b1babd91fe71567a0d41edca47e6b3b137d9e9a3fb93474adafd`,
source content `0bb7251e3e4246d43af2be1f1886059a70c39bbe3d5a584bd3ccf8a60373a527`,
and selector `packaged_crypto::packaged_fresh_snapshots_pass_independent_age_signature_inventory_and_negative_controls`.
It reports stable source, exit zero and no timeout under its 600-second bound;
the result and five referenced files were rehashed and matched. Its retained
`crypto-report.json` has SHA-256
`71e0febc73304b9dfce8c61b08e3c2a8859e55caf62d8ea544f722fb1444e812`.
Both report cases identify the exact `0087829f` executable. Independent Go age
v1.3.2 has executable SHA-256
`2821a4ed191da07372acd302e5f6feae7a7985e285e1417765ebe74025af45f0`.

The independent verifier checks 203 payloads, 99 command scopes and 111 event
scopes per backend, including five task/verification/ledger baseline records.
Payload bytes verified are 774,879 for SQLite and 774,318 for Files. Local
opposite-backend activation verifies 71 artifact payloads, 136 transactions,
99 commands, 111 events and four source files per case. Completed tasks remain
completed; unfinished tasks remain paused, authority is Untrusted/Plan without
grants, and exact import retry changes nothing. Each case records three
synthetic loopback requests and zero paid or maintenance provider requests.
This crypto report explicitly does not establish cloud transfer,
independent-machine recovery or complete P8-03 by itself.

The debug startup/history cost has a concrete source-level scaling explanation,
not a measured timing attribution. Both [backends](../../src/crates/vcp-store/src/backend.rs)
replay every retained commit through
[`State::prepare` and validation](../../src/crates/vcp-store/src/contract.rs),
which clone/serialize accumulated state and validate its records, references,
accounting and events. [Live transactions](../../src/crates/vcp-store/src/store.rs)
also use that path; reopen verifies all retained artifact bytes, followed by
another spool scan during lifecycle startup. The
[CLI history path](../../src/crates/vcp-cli/src/app.rs) opens Store separately
for the query and retention notice. This is consistent with retained diagnostic
observations of roughly 22–24 seconds for inspect and 44–47 seconds for history
as canonical history accumulates. In the final larger retained fixtures,
revoked-owner runs take about 200 seconds, purge preview/apply about 184–191
seconds, and each final purge inspector about 178–180 seconds. All remain within
their predeclared correctness bounds. No performance fix, timing profile or
latency acceptance is claimed by this qualification increment.

The final fast contract gate also passes all 17 groups in 40.655 seconds:
`artifacts/p80203-corrected-fast/3e4695a5-c091-4466-bc8a-a9dec48fbc39/manifest.json`,
SHA-256 `3f28c2e53a2e0fcd862d62b0c62d2a8eb2d776b560981d919b3290cacf915410`.
All 34 referenced logs were rehashed and matched. This gate is delivery-harness
evidence, not an additional product or physical power-loss qualification.

The focused positive startup regression has result SHA-256
`6a210e55d4a3776f93cd583fa4c74c59b4d207656978a4f3323bc7ea00220079`,
source content SHA-256 `d8dd677dfe77809f0815f092c62497bd5069dee211500adb2502f9493f2a7ea4`,
and selector `acknowledged_partial_capture::exact_acknowledged_partial_output_allows_new_owner_admission`.
Its result and five referenced files were rehashed and matched. It is a native
component pass, with its original source identity retained; the later negative
fixture correction does not relabel that source inventory.

The negative guards and existing capacity regression both pass against source
content SHA-256 `71e9b567a1e078719965d8354508e1428ade99c783a11e5e587e40ed004ecbdb`.
Their result hashes are respectively
`696ec7f0d9580da78041552cf343ef7bf5a848ce011714d4da93be2cd1cf32a2`
and `8cd8d6ced50f5237db5653f6f82a2fd853dae7ab74a876a620aaf38cdd5d07ae`.
Each result and its five referenced files were rehashed and matched. The exact
selectors are `acknowledged_partial_capture::unacknowledged_pending_mismatched_failed_and_provider_captures_stay_fenced`
and `real_capture_capacity_failure_fences_transport_and_preserves_exact_prefix`.
The post-correction four-arm MCP owner-kill regression also passes; its exact
receipt is recorded in the [recovery follow-up](p8-local-recovery-followup-2026-09-22.md).
Those four repeated kills do not increase the count of distinct qualified crash
positions. The rebuilt packaged MCP history receipts above independently satisfy
their complete selected schedule.

## Preserved attempts and review

`sensitive_stage-d750fc1c-8386-48ae-aa63-76af9393cb20` stopped before capture:
the fixture passed an empty forbidden-directory list to `RecoveryDirectory`,
which correctly returned `Access`. The correction supplies the actual sibling
staging/canonical exclusions; no access control was weakened.

`sensitive-2e9f98f8-1cfc-4228-afa2-b94431e2a402` reached the chain inspector and
failed its single-page assumption. The correction traverses all pages with a
finite bound and scans every reply. Independent review also required rescanning
the original key-create stdout/stderr after the generated exact writer seed
becomes available, closing an oracle gap for a seed emitted without its prefix.
These attempts do not demonstrate a product secret leak and remain recorded.

All listed failed result receipts and their five referenced files were rehashed and
matched. Under `artifacts/p80203-recovery/`, their identities are:

| Attempt | `result.json` SHA-256 | Source content SHA-256 | Wall / test seconds |
|---|---|---|---|
| `sensitive_stage-d750fc1c-8386-48ae-aa63-76af9393cb20` | `cce0ad270ded6a15ad6d2e782110ffb6435d5545ca556e7e5401631b41195415` | `97f095829cea532028a5a7bf248b04cde6505e7a010cd53be64f179f95daf737` | 6.508 / 0.00 |
| `sensitive-2e9f98f8-1cfc-4228-afa2-b94431e2a402` | `0ce1ff0dd7b8ee26e389f18b2b0c35da19be42845792c467f414c3c23fcfec0d` | `97f095829cea532028a5a7bf248b04cde6505e7a010cd53be64f179f95daf737` | 54.370 / 48.81 |
| `mcp-67e835ff-4708-4085-9cca-bba44c5646e9` | `3a87c3403678841e3d586b3ad3c6ed7985ae83ede59276f982b4f7047d516fc9` | `e13e588942e462c3406e6116921596590458c8ccd29f63b123bcd8ca5fb7beec` | 530.659 / 525.85 |
| `mcp-d28c5a20-30ca-40cb-8d76-a45f6320c600` | `d3314454de963cdc8d107acee36d5a0d01cdf653269eb7940d8de65d785fffd9` | `274de89ccbb1226d11ffec8346ebcfb818c453dc74c7bae7c5c309010bf43cc2` | 229.033 / stopped before completion |
| `partial_ack-058a5d8c-dda0-4875-b359-de648c20c837` | `e62d4e795c54bd8fa7937ce45e1ba545db684127a05bf7f89f7d4cf2a3259f1c` | `8ef591c9e3e8508a406c2b5a495098208d5f596d3e09bab2abd2c8122cc68173` | 8.677 / compilation failed |
| `partial_ack-568c067c-11e7-4d5a-97c8-44087042ed4b` | `180e83b66d9b6d7d89043c5200cb6d484dff74daeebd93dac6327ac0357d00a6` | `a91a524dcb47cdabfc132fa06b3a8e3c4f96ece2e951f78f5e52ccc168b427b1` | 33.278 / compilation failed |
| `partial_guard-912d7df0-f92e-40c9-85f6-3c46fdeaf6d0` | `2875013a287a202aa53c0d71f33e29930a6601fe737b58dad45eaa4e3c912cd1` | `d8dd677dfe77809f0815f092c62497bd5069dee211500adb2502f9493f2a7ea4` | 5.222 / 0.37 |
| `build-e66de150-08bf-4dfd-8839-0e1d5d01844c` | `3f914649d85d7f9fac95392067400899be44fc7bfacfefa933fce78fc525b66b` | `71e9b567a1e078719965d8354508e1428ade99c783a11e5e587e40ed004ecbdb` | 33.745 / compilation failed |
| `mcp_sqlite-1e314476-6ea0-46f5-8dd1-88576fd0010d` | `ff90a560920420b6a36e20715ca26da7aeb41e8083148fcb9045e541ee93ec5c` | `0bb7251e3e4246d43af2be1f1886059a70c39bbe3d5a584bd3ccf8a60373a527` | 668.225 / 666.51 |
| `mcp_files-c2245e39-6f9a-466f-90d0-fc28db0cbda7` | `61d86d89b31605a444e675fbf734114cb969b3f712ed6263994a5ab0dcceeb66` | `0bb7251e3e4246d43af2be1f1886059a70c39bbe3d5a584bd3ccf8a60373a527` | 661.839 / 660.76 |
| `mcp_sqlite-a1323d9e-4f82-47c1-9eae-8b314a665500` | `ae60ba4fddd7a368caf29f977218a8045639be43076412c059117af97b7049bc` | `fba77faae80fa379c918db27bae97505c5038352bffd50b9ed1cf4fcd6b4f969` | 806.933 / 805.22 |
| `mcp_files-8cc78b59-3ba4-4d98-ace8-1d469c167c55` | `43da08c24163e6eeba5a2ad0c6d604fbd68c7f515e449b9835cd395612c825f1` | `fba77faae80fa379c918db27bae97505c5038352bffd50b9ed1cf4fcd6b4f969` | 810.989 / 809.86 |

The two split MCP failures each retain roots and per-command diagnostics. Both
advanced beyond the corrected startup-classifier boundary and through provider
request 106, but their first fresh-owner CLI command was killed at its inner
90-second deadline. Each outer result reports exit 101 and `timed_out: false`;
that field describes the outer supervisor, while the retained command
observation explicitly reports its inner timeout. Their result and all five
referenced files were rehashed and matched. Neither is semantic qualification
of the full MCP history/compaction/reopen/purge schedule.

The next split attempts (`a1323d9e`/`8cc78b59`) completed the first fresh owner's
checks, then hit the inner 180-second deadline at CLI command 17 after provider
request 206. Both retain empty CLI stderr and canonical progress through a
`verification-plan/1` artifact. The scripted provider emitted the intended
sequence once: stale-cache read, discovery, current resource catalog, old
resource read, disconnect, ordinary patch and verification. It had not received
the expected next request for its final response; no responder index loop was
observed. SQLite's new-task event interval was 82.598 seconds, with about
89.708 seconds between the prior task's completion and this task's creation.
These observations distinguish substantial startup/history-processing cost from
a demonstrated verification deadlock. Both result receipts and their ten
referenced files were rehashed and matched; `timed_out: false` again describes
the outer bound, not the expired inner CLI bound. Neither attempt qualifies the
remaining revoked-authority or purge assertions as passed.

The first classifier build failed on a Result/Option conversion; the next failed
on missing test trait imports. Both were corrected before the positive pass.
The initial negative-guard fixture incorrectly expected Store to accept a
mismatched canonical descriptor: Store already rejects it with
`Corruption("artifact descriptor changed")`. The corrected test asserts that
refusal, then verifies the resulting unacknowledged physical artifact still
fences admission without changing its bytes. The selected `9a8ebec9` rerun passes.
The subsequent `e66de150` no-run build failed on the split test wrapper's borrowed
backend lifetime; the wrapper now accepts the actual static backend labels as
`&'static str`. The selected `9016af96` build passes after that test-only fix.

The first packaged MCP history attempt completed initial capture and compaction
assertions, then failed at the first fresh owner run (`revoked=false`) with
`canonical capture/admission fenced; reopen required`. It exited 101 without
timing out. Its result and all five referenced files were rehashed and matched.
The original fixture roots were automatically removed, so this receipt alone
does not identify the underlying fence cause.

The retained diagnostic rerun `mcp-d28c5a20-30ca-40cb-8d76-a45f6320c600` observed
two `retained-full-output/1` artifacts immediately after the successful initial
SQLite owner exited: both `Aborted`, with lengths zero and 1,064 bytes. Those
deliberately stopped process outputs were canonically acknowledged. Startup
classified every non-Complete physical capture as interrupted, including those
exact acknowledged partial outputs, and consequently fenced the next owner.
The diagnostic was explicitly stopped after identifying this product defect;
its exit -1 is a failure/incomplete observation, not a pass or deadline expiry.
Its `diagnostic-stop.json` has SHA-256
`753fe215a235512491ab9d3ef3dc61d07a5cce055be4cf6552c9276d4d228480`.
The result and all five referenced files match their recorded hashes.

The scoped correction permits only exact canonically acknowledged Aborted
`retained-full-output/1` stdout/stderr/child-transcript descriptors without a
`CaptureFailure` omission. Physical Pending captures, missing or mismatched
canonical descriptors, failed captures and provider captures retain the startup
fence. The global unfinished-spool API is unchanged. The focused both-store
positive and negative regressions pass, along with the existing capture-capacity
and four-arm MCP kill regressions. The rebuilt packaged MCP history cases now
pass as well. No component or package pass relabels a package built before the
fix.

## Limits

Authenticated HTTP MCP credential/session exclusion retains its native evidence;
the new packaged MCP history fixture uses stdio and makes no authenticated HTTP
claim. There is no diagnostic-bundle export command to qualify. Ordinary
token-looking source text is intentionally retained under capture scope; this
does not promise heuristic secret detection or erasure of external copies.
Production builds, clean Windows installation and owner acceptance belong to
the remaining P8-01/P8-04/P8-05 work. Independent-machine recovery is not run.
