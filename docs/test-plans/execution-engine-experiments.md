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

## Live qualification and analysis

After integrated offline checks and resolution of the outstanding campaign-spend authorization question, build one synchronized new local candidate and run a small real repair through the same launcher/engine. Analyze both successful and failed outcomes, then run full A and B with their original independent gates and follow with larger full engagements. Retain all artifacts and observations. Record any constraints still active; a harness-only change is not evidence of full engine suspension. Do not silently count interrupted, blocked or incomplete scenarios as passes. No code is pushed, published or released by this workflow.
