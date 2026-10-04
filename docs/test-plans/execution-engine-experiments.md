# Execution engine experiments

Work items: EE-00c, EE-06 and EE-07. The [implementation ledger](../plan/25-execution-engine-refinements.md) records completed checks and outstanding acceptance. Experiments exercise the existing engine. A test helper does not provide a second execution path or authority to dispatch model requests.

## Offline reconstruction

Collect an authorized `vcp inspect-bundle <task>` result from the selected locally built candidate. Preserve the original JSON/JSONL, scenario scorecard, candidate version/hash, source revision and dirty diff, provider/model setup, backend, workspace and independent check outputs. An inspection after reopening measures that inspection owner; its phase counters are not the earlier execution's lifetime counters. Missing earlier spans remain unknown.

Run the offline analyzer on one or more retained bundles. It performs no model requests, creates a new report exclusively and refuses to overwrite previous analysis:

```powershell
node scripts/evals/analyze-execution-bundle.cjs --out artifacts/new-analysis.json path/to/inspection-bundle.json
node --test src/tests/contracts/execution-analysis.test.cjs
```

The report separates recorded causal edges from observations sharing explicit record identities. Neither time ordering nor a shared artifact alone proves causation. It retains missing/redacted evidence and unknown liabilities, and requires independent quality review. `retained_with_omissions` can describe intentional authentication-header/recovery-material omission; it does not automatically mean the execution evidence is missing.

The scenario harness now creates an immutable bundle snapshot, an offline report and a hash-linked `execution-analysis-index-*.json` for each inspection, including failed task outcomes. Stage scorecards reference these indexes. Repeated collection uses new filenames and preserves prior reports. Missing Node, invalid evidence or analyzer failure is recorded as unavailable and added to the scenario notes; it does not change the observed task outcome or authorize another attempt. The collection integration is checked by `pwsh -NoProfile -File docs/test-plans/tests/ExecutionAnalysis.Tests.ps1` without a provider or CLI process.

When lifecycle observations are available, the analyzer reports sample counts and nearest-rank p50/p95 separately for each phase and status. Active durations are elapsed-so-far, interrupted spans are incomplete, and a single sample is labeled accordingly. Dropped observations and owner-window limits remain explicit. Overlapping phase durations are never added into a fabricated controller-overhead or end-to-end total; store cumulative counters cannot supply missing percentiles.

For every material experiment, write the expected behavior, observed result, failed or unrun independent criteria, evidence joins, explanation supported by those joins, competing explanations and the next discriminating change. Compare timing only with candidate, workload, backend, host and cache conditions recorded. Costs and speed do not substitute for correct outputs. A successful tool command does not prove native completion, and native completion does not replace the scenario's independent gates.

## Retained baseline analysis, October 4, 2026

The offline analyzer was exercised against two original campaign bundles without modifying them. These are prior failed runs, not qualification of the new code.

| Retained task | State | Events | Explicit causal edges | Attempt identities | Effect identities | Settled micro-units | Unresolved micro-units |
|---|---|---:|---:|---:|---:|---:|---:|
| A T5 `281a22c7-f1ed-4e55-937f-a513b48b674b` | paused | 486 | 0 | 18 | 11 | 588481 | 841960 |
| B repair `f35a91bf-9e65-4f52-b03b-e6c68baf74df` | failed | 846 | 0 | 27 | 22 | 1379737 | 0 |

Source SHA-256 hashes: A `0c91410a3c75c1624d016921d6f5092913ccd052586005fd14ebd932c0e68ce1`; B `35a9bd1fd4c3205e859045dd91ad5698961013d36570fa90a25adb33d1a56d5a`. Local generated analysis: `artifacts/ee-baseline-bundle-analysis-v2.json`. The first report is retained separately. No verification records appear in either selected bundle. This supports a specific diagnostic improvement: preserve and expose attempt/effect/verification identities and owner repair decisions rather than assuming event count or timestamps can reconstruct the chain. It does not by itself identify the provider failure cause or prove the B application met all requirements. A's unresolved charge remains unknown; B's settled accounting does not make its failed task successful.

## Scripted qualification before live comparison

Use local mock providers and both storage backends where the affected boundary supports them. Relevant cases are missing/stale native verification, actual failed check followed by a source-changing repair and fresh successful completion, instruction refresh before reissuing tools, denied process authority, repeated unchanged failure pause, explicit cancel, stop/reopen with unresolved effects, oversized tool output, changed/deleted file ranges, scoped artifact access and a truncated response whose partial tool call must never execute. Record actual request bodies and retained source/response references; returned helper strings alone do not prove the model received repair guidance.

Small-capacity fixtures must retain their declared limits. A larger fixture must explicitly normalize its larger provider catalog and use normal request admission; increasing test capacity is not a production capacity fix. A truncated response needs one-use continuation evidence, a concrete allocation or progress change, and a bounded no-progress stop. Focused verification remains diagnostic; full applicable checks and fresh authority remain required for completion.

### Shared-driver repair reconstruction

The production CLI `RetainedExecution::start_completion` and its single event owner were exercised with a local scripted provider on both backends. The objective was to change `value.txt` from 41 to 42 and pass the independent Node assertion. The scripted first patch wrote 43. Owner verification returned exit 1; its retained verification ID appears in the controller repair artifact and the actual subsequent encoded request contains the failed-check guidance. The next scripted patch wrote 42, a fresh full check returned exit 0, and the same task reached Completed. The test then closed all owner references, reopened read-only, verified retained diagnostic evidence and observed no new provider request. This proves the tested control flow and disk/check result; it does not evaluate a real model's repair ability or qualify A/B.

Both runs retained 161 events, four mock provider requests, one repair decision and failed/passed native verification records with different source fingerprints. Each archive preserves 98 complete same-task artifacts, including exact request bodies, context manifests, repair feedback, verification source/check output and embedded timing snapshots. Synthetic settled accounting is 400 micro-units per run; this is fixture data, not paid spend. Missing token subcounts remain unknown. The four captured allocations classify planning, editing and repair; they preserve the 4,096 output allowance, so these runs do not establish token savings or the adaptive shrink rule.

| Backend | Archive directory under `artifacts/execution-engine/scripted-repair/` | Bundle SHA-256 | Manifest SHA-256 |
|---|---|---|---|
| Files | `Files-repair-a9617696-22a5-4034-bd6a-423d6f51cb90` | `9f0ab8412653fad132beeef2bdac87886bf8185699edd7c6a0b3bc1fc3ce3f82` | `8dab7158c665fda141838179007a73602a7ec78a5495182a894605066b84c205` |
| SQLite | `Sqlite-repair-bcadc922-7734-492e-b9b1-6d5237abf757` | `14c171769650151b7d11905119712f1edaa7911d4d55150e0a13e13c18f4b684` | `3bf1035b62a2684c6aaf6f37da3b57504b21c0234952b8cbbd5f6eca865b0bc0` |

The offline analyzer accepts an archive directory as an input. It checks manifest/bundle hashes, every included descriptor against the authorized bundle, exact scope, content hashes and byte bounds, then extracts repair-to-verification identities, request allocations linked to captured request hashes and durable diagnostic checkpoints. It rejects escaping paths, conflicting descriptors and changed bytes. Supplied hashes establish internal consistency, not independent provenance or execution authority. Generated analysis is retained at `artifacts/ee-scripted-repair-archive-analysis-v2.json`; the first archive report and earlier bundle-only reports remain separately retained.

No event supplies an explicit causation parent in these runs. Reconstruction therefore uses the recorded verification/artifact/request identities and fixture assertions, with event order as observation, rather than inventing causal edges from timings. The 178 artifact visibility notices per bundle are declared authentication-header/recovery-material omissions; the captured safe bytes remain inspectable. Timing snapshots name their owner and report partial history; a verification snapshot captured inside its own required report can correctly show that span still active. The final live snapshot and prior embedded snapshots must not be summed or treated as one complete lifetime stream.

These tests used a changing local source tree and synthetic endpoint, not a frozen distributable candidate or matched A/B benchmark. Exact candidate/host/cache provenance and live quality comparison remain required in EE-06/07. The next supported actions are stop/resume regression qualification, completion of the blocked constraint integration, then the small real repair and full workloads after authorization is resolved. The evidence supports controller-owned re-verification and actual feedback delivery; it does not support a claim that the broader architecture is qualified.

## Live qualification and analysis

After integrated offline checks and resolution of the outstanding campaign-spend authorization question, build one synchronized new local candidate and run a small real repair through the same launcher/engine. Analyze both successful and failed outcomes, then run full A and B with their original independent gates and follow with larger full engagements. Retain all artifacts and observations. Record any constraints still active; a harness-only change is not evidence of full engine suspension. Do not silently count interrupted, blocked or incomplete scenarios as passes. No code is pushed, published or released by this workflow.

The larger [TaskBoard data-recovery and Inventory stock-audit workloads](execution-engagements.md) define expected behavior and independent acceptance before execution. Both remain unrun. They deliberately require repeated changes across files, a failure/repair cycle, explicit pause/read-only reopen/resume and preservation of the original A/B gates.
