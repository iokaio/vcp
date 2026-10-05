# Execution diagnostic fault reconstruction

Work items: EE-00b/c and EE-06. On October 4, 2026, the production CLI session, shared event owner, canonical tools, request admission and capture paths ran two local scripted cases on both SQLite and Files. The [fixture](../../src/crates/vcp-cli/src/execution/tests/diagnostic_cases.rs) passed all four runs. No paid provider was used, and finite admission/deadline settings remained enforced. This is diagnostic qualification, not application-quality or A/B qualification.

## Retained evidence

The scoped archive exporter reads artifacts through the owner's existing authorization and hash checks. Each new directory includes the inspection bundle, descriptor-bearing manifest, manifest hash and captured bytes. Sealed Aborted response prefixes are retained with their original partial state; open captures are excluded. The offline analyzer verifies all included bytes and cannot interpret partial JSON as a completed repair or request. Earlier archives remain unchanged.

The following directories are under `artifacts/execution-engine/diagnostic-faults-v2/`. `artifacts/ee-diagnostic-faults-analysis-v2.json` contains manifest hashes, verified artifact counts, phase samples and explicit evidence gaps. The earlier `diagnostic-faults/` archives and first analysis remain retained; those archives omitted the partial response bytes, which motivated the exporter correction.

| Backend and case | Archive directory | Bundle SHA-256 |
|---|---|---|
| Files, context omission | `Files-context-omission-fc6952b4-36c5-4a37-9977-ac01111eaa59` | `ee14c2536719fd9a9ae5fa85f7139fee3796b6c22085601fae5bdef022ce73e0` |
| Files, interrupted provider | `Files-interrupted-provider-e83cdbb8-57b6-499a-a0ad-49bb19469602` | `1dec11c8312f39b104cb54924ae221f39e2684ae8d495f414e8713456dde4f4e` |
| SQLite, context omission | `Sqlite-context-omission-70d78bd2-5e59-43cd-87c7-4bac5352b589` | `125c258fb2dca075daaa93b3cb04a045f88ce0478f9fdcb8670df226b6fe6dbe` |
| SQLite, interrupted provider | `Sqlite-interrupted-provider-c25b4b23-11c1-49d8-83d4-606c5d4fe136` | `375a90a03df26f86c5fd7995006ff5bc4d77fa61d0c01b05e7022ab773ad2fc2` |

## Context omission

The scripted provider requested a real native read of a 120,024-byte file. A distinctive middle marker was present in the retained complete tool result but absent from the next exact provider request. The canonical compaction projection records one compacted pair, source artifact identities and hashes, its summary artifact, before/after encoded-request hashes and a conservative byte estimate of 141,207 before versus 22,417 after. Both backends report the same 118,790-byte difference. The request explicitly carries `omitted_bytes`; its manifest does not claim a selection exclusion because the omission happened earlier in the bounded pair projection.

This joins the original source/result, the declared projection and the actual later request. It confirms that bounded context caused the marker's omission while the original evidence remained retained. It does not prove that a real model could recover the marker or finish a larger task. The fixture explicitly pauses after observing this boundary; it does not claim completion. Each run retained 40 complete artifacts and 66 events, with two provider requests, one read effect and 200 synthetic settled micro-units. ToolDispatch records one completed wrapper operation; canonical effect evidence separately confirms the read outcome.

## Interrupted provider

The local SSE source ended after a patch-argument delta, before any terminal response. The archive preserves the exact 454-byte Aborted prefix, with SHA-256 `bc15466b6f9bddc5dd426964edc6f760281acb7cf0227faabd4dec7fe5f6b9b2`, and its omission declarations. The failed-provider-request artifact joins the attempt and raw-response identity. The request, reservation, partial response and final Paused task can therefore be reconstructed without the live session. The prefix contains the argument delta and no `response.completed`; no normalized completed call or effect was published, and the workspace value remained unchanged.

Each backend retained 21 artifacts, including that partial prefix, and 32 events. There was exactly one provider request, no tool dispatch, zero settled charge and 100 synthetic unresolved micro-units. The unresolved amount is retained liability, not a final bill or free request. ProviderExchange is explicitly Interrupted. The fixture establishes deliberate local EOF as the initiating condition; these artifacts alone would not identify an upstream network failure's physical cause.

## Interpretation and remaining gaps

Phase statistics separate completed, failed, skipped and interrupted observations. ToolDispatch success means the wrapper finished processing; it does not imply that a process exit code was zero or a verification passed. Instruction-scope refresh is separately exercised by the lifecycle fixture as Skipped followed by a later completed dispatch. The offline analyzer's five contract tests pass, including partial-capture integrity, escaping-path/corruption rejection and exclusion of Aborted JSON from completed-repair reconstruction.

Event causation remains absent in these runs. Reconstruction uses explicit attempt/artifact/request identities, source hashes and fixture assertions; ordering alone is not a causal edge. The context runs retain 51 visibility notices for intentional authentication/recovery omissions; interrupted runs retain 22 such notices and one partial-capture notice. Those declarations are preserved rather than treated as missing authentication evidence to recover. Current-owner timing windows do not reconstruct earlier owner lifetimes. These earlier ToolDispatch spans identify the originating attempt but have no sibling call ID; the follow-up below supplies that evidence without rewriting prior captures. These small unmatched samples do not establish latency improvements. Real-provider behavior and full A/B application gates remain separate qualification work.

## Exact sibling-call timing joins

The follow-up `archives_sibling_tool_calls_with_exact_diagnostic_identities` fixture passed on both backends. One response supplied two native read calls, `read-large` and `read-sibling`, under the same attempt. Two ToolDispatch observations now carry those exact call IDs, the same attempt, current scope and turn. Each observation is matched to its canonical completed pair by both call ID and attempt. Their observed dispatch order differs from the response order, so this proves identity joins rather than an assumed positional association. The span stores no tool arguments or output. The optional field is omitted from legacy/non-tool observations; empty, control-containing and over-256-byte IDs are excluded, and offline analysis rejects invalid supplied IDs.

Each run retained 45 artifacts and 77 events. Archives under `artifacts/execution-engine/sibling-tool-diagnostics/` are `Files-sibling-tool-calls-d514593b-2dff-44b9-97f3-9832a8f9c22c` and `Sqlite-sibling-tool-calls-558d9eff-8884-461e-8def-4353456a562e`. Their bundle SHA-256 hashes are respectively `24bfe7219955aa9bbb5a90a340886e15e28def2d4b9c902dd52c7fee144d5f65` and `a3cc8f7e7122addffbdcbaedc0e2862b03953c5575a590f7ae1287345151a6a8`. The verified offline report is `artifacts/ee-sibling-tool-diagnostics-analysis.json`.

## Stale verification refresh and repair

The `shared_driver_refreshes_stale_verification_and_exports_evidence` fixture passed on both backends using the production CLI session and shared event owner. The scripted provider patched the value from 41 to 43, invoked native verification, then performed a native read after that report. Completion returned the typed `StaleVerification` rejection because the later effect invalidated the report's freshness. Both recorded source hashes still describe `43\n`: this exercises conservative freshness after a later effect, not a changed-source case. The existing completion path ran verification again, retained a fresh failed report, submitted repair feedback in the same task, accepted the patch to 42 and completed only after a fresh passing check. Read-only reopen caused no additional provider requests.

The fixture captured its observed typed rejection through the ordinary evidence API as a `retained-output/1` artifact, explicitly labelled as a local qualification assertion without execution authority. The archive names this artifact in `fixture_observation_artifacts`; the analyzer requires its complete, scoped, hash-verified bytes and joins its prior report to canonical verification facts. This records a tested observation without inventing a production lifecycle event or an independently attested causal edge. The repair artifact references the fresh failed report, rather than the earlier stale report, and the final report records exit zero. Each run retained 142 artifacts and 232 events, six mocked provider requests, 600 synthetic settled micro-units, no active reservation and no unresolved charge.

| Backend | Observed stale report | Refreshed failed report | Repair artifact | Final passed report |
|---|---|---|---|---|
| Files | `3b4411a5-5643-4e9b-86f1-68492396ad11` | `51db7b1f-9e3c-4378-bdec-c393fb798adb` | `f42118d0-c6b1-4185-9c08-3dd5987c9d27` | `d75e08e5-30f5-4256-a726-ebeacb5feb90` |
| SQLite | `b01cdf3e-557c-4daf-ac63-16e2da5cae4c` | `e8034696-8460-47a7-bbc2-cfd9b755e341` | `c9833b49-aa8f-492e-81b8-8e72999a7a65` | `b1177171-5102-433e-8164-f185f97a38c3` |

Archives under `artifacts/execution-engine/stale-verification-diagnostics/` are `Files-shared-driver-stale-verification-refresh-91e9958e-7b2b-453a-a1b7-b06779b79a3c` and `Sqlite-shared-driver-stale-verification-refresh-57e2abf5-5b56-49ae-9c33-757c5aa9931d`. Their bundle SHA-256 hashes are respectively `c638a2d378eb0507008b20d5b5579a9d81f38aa3716c04b58fd1da8e71305c11` and `e20ce935aa224c1d4389acdde43b6649d39133b60c1dc5d6bea1bec26e46fe6a`. The verified offline report is `artifacts/ee-stale-verification-diagnostics-analysis.json`.

## Interrupted native process, cancellation and reopen

The `archives_interrupted_native_tool_and_preserves_cancelled_reopen` fixture passed on both backends. The scripted provider invoked real `vcp_exec` with an isolated Node profile. The process recorded one dispatch, wrote distinct 29-byte stdout and stderr prefixes, then waited indefinitely within its finite process timeout. After its readiness marker, the host explicitly cancelled the task through the normal stop command. Repeating that exact command returned its original receipt. Reopening the canonical owner preserved Cancelled, the same effect/execution identity, both retained prefixes and exactly one provider request and native dispatch.

This experiment exposed and fixed an evidence defect: the owner terminated the native job without recording a stop reason in its shared output control, so the broker labelled interrupted streams complete. The job registry now associates the exact job with that control and records `owner interruption requested` before termination, preserving any earlier deadline/output-limit reason. Every job is still terminated and drained when one interruption-metadata write fails. Two focused native-process regressions passed, including two sibling processes with the first control deliberately poisoned; independent review found no remaining concrete lock/order issue.

In both retained runs the broker observed termination and job quiescence, so the precise effect outcome is Failed with exit code 1, rather than OutcomeUnknown. Its complete outcome receipt says `output_complete:false`, records zero owned processes remaining, and joins to both Aborted stream artifacts through the effect's explicit `observed_changes`. Both streams declare `explicit_abort`. Stdout SHA-256 is `74fceeb142a99f3befed73437391dfe3f940cd5ee8b3960c850ac9a6b23f355e`; stderr SHA-256 is `25da4cb9a5ee97fd29e3fe244d82cef0afe41a99dbeb8cb295eb441189983fad`. Each run retained 31 artifacts and 51 events, with 100 synthetic settled micro-units and no active or unresolved provider charge. The provider response completed before the tool was cancelled.

| Backend | Canonical coding pair | Failed effect | Partial process outcome |
|---|---|---|---|
| Files | `b18dde27-e1b5-410d-bbbc-82fccd9eb594` | `bd6c7f14-20c5-493e-8672-07d370b9f518` | `da5e0601-c15a-49ff-aae3-8ebe00a6bdb7` |
| SQLite | `586e13a9-8521-43c3-af59-5a213b1fdad0` | `25bba11c-08c1-4bee-a761-f25f4f9522b9` | `81151abe-8b6c-459a-ab4a-e28232bf5146` |

The pair joins the ToolDispatch attempt and `interrupted-process` call ID to the returned effect/outcome/stream identities. ToolDispatch is Succeeded because wrapper processing and result publication completed; the process itself failed and its output remains partial. This is concrete evidence against treating a successful timing span as a successful operation. The analyzer now exposes canonical effect state, explicit stream references and process receipt joins, while continuing to exclude Aborted JSON from completed outcomes. All seven analyzer contract tests passed. Missing effect references remain explicitly labelled, never inferred from proximity or order.

Archives under `artifacts/execution-engine/interrupted-tool-diagnostics/` are `Files-interrupted-native-tool-90c2b0a5-deb1-414d-8284-a792e07f8a17` and `Sqlite-interrupted-native-tool-525b8493-7dc5-4cb4-a28c-7b19538d7a0e`. Their bundle SHA-256 hashes are respectively `029883602c6c0de5a557862f8c53ab07420cf422ea1738ce8775e73b18982204` and `83cc62628706cf0a96e19b6f01970c527c9cc238387801076bc04b94059214a1`. The verified offline report is `artifacts/ee-interrupted-tool-diagnostics-analysis.json`.

The separate existing `child_native_write_process_kill_retains_unknown_effect_without_replay` fixture also passed on both backends at this source integration. It kills the owner between a real partial native write and its receipt, verifies OutcomeUnknown after reopen, preserves a later human edit, rejects unsupported completion and proves no provider/effect replay. That fixture supplies the unknown-outcome fence evidence; it does not turn these deliberately cancelled, observed process outcomes into unknown ones, and no standalone archive is claimed for that separate run.
