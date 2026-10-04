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

Event causation remains absent in these runs. Reconstruction uses explicit attempt/artifact/request identities, source hashes and fixture assertions; ordering alone is not a causal edge. The context runs retain 51 visibility notices for intentional authentication/recovery omissions; interrupted runs retain 22 such notices and one partial-capture notice. Those declarations are preserved rather than treated as missing authentication evidence to recover. Current-owner timing windows do not reconstruct earlier owner lifetimes. ToolDispatch spans currently join the originating attempt but do not separately identify sibling call IDs, limiting per-call timing attribution for parallel responses. These small unmatched samples do not establish latency improvements. Interrupted native-process/tool evidence, real-provider behavior, verification-refresh archive reconstruction and full A/B application gates remain separate qualification work.
