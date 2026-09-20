# P6-04 offline routing assertions

`manifest.json` declares the task pool, partitions, strategies, quality gate,
sample requirements, task caps, retry limits, supporting-cost components,
intervention limit and expected eligible/selected identities **before execution**.
The fixture contains three tuning cases and six held-out cases. Each partition
contains analysis, review and generation tasks. The revision is
`p6-04-offline-routing-v1`; altering the criteria or expectations after seeing
results requires a new revision and an explained rerun.

This is a deterministic policy harness, **not a model task-quality comparison**.
The task prompts are fixed inputs for decision assertions; they are not submitted
to a model or solved. Quality, prices, samples and latency in the two fixture
candidates are explicitly synthetic. A passing result proposes no production
model pool, threshold or shipping default.

## Run

From the repository root with its provisioned native Rust environment:

```powershell
pwsh scripts/evals/routing-qualification.ps1 -Jobs 4
```

The runner creates a unique directory under `artifacts/p6-routing-qualification`,
records source identity before and after execution, and retains toolchain,
hardware, commands and hashed logs beside the report. A
nonzero exit preserves the report when a selector assertion or setup fails.
Keep the full output under ignored artifacts; publish only reviewed summaries.
The report records the manifest, harness and production routing/catalog source
hashes, normalized immutable catalog/policy payloads, hardware basics, execution
order and every attempted decision. The enclosing qualification run should also
retain its source revision, toolchain, command and log.

The example has no network client, remote dispatch or model-call code. It uses
the real `Snapshot::from_endpoints`, `CatalogRevision`, `Policy` and `select`
implementations. There are no changes to production eligibility for the harness.

## Two separate assertions

First, `scripted_evidence_rejection` builds records with the actual `Scripted`
evidence kind. All three strategies must decline selection and each candidate
must retain a missing-live-qualification reason. This proves that scripted
observations cannot enable a real model default.

Second, `hypothetical_qualified_fixture` uses in-memory counterfactual records with
the production `Live` discriminant to exercise selection conditional on valid
qualification. These records are explicitly marked synthetic, never published
to a canonical registry, and have fixture dates within the first second of the
Unix epoch. They are not observations of actual live providers. This phase
tests the same guarded branches as production routing unit fixtures without
weakening the live-evidence requirement.

Both phases run the following three strategies over identical task inputs and
candidate data, with isolated decision inputs for every attempt:

- `fixed_economical`: strict pin to the economical synthetic candidate; no fallback.
- `fixed_stronger`: strict pin to the stronger synthetic candidate; no fallback.
- `routed`: the declared low-profile ordering over the eligible synthetic pool.

All strategies keep the same quality floor, allowed-provider/model restrictions,
support assumptions and task cap. A fixed strategy can correctly stop when its
candidate violates the gate; it never gets a waiver to improve a comparison.
The recorded independent expectations cover cheap-but-below-floor review,
expensive retry history, excluded stronger identity, protected completion reserve,
unknown support cost, expired metadata and unknown tool support.

The tuning partition is reported separately and no parameters are fitted during
execution. The held-out labels are only consumed by the assertion grader after
production selection; candidate construction uses the separately declared model
cohorts. No outcome-driven threshold changes, repair attempts or human
interventions are allowed in this revision.

## Costs, timing and results

Every row preserves the full production decision, candidate exclusions,
first-attempt/retry/handoff estimates, support/child/verification estimates,
assumptions, pin and chosen identity. Helper, compaction and optimizer components
are predeclared separately in the manifest and attributed in cost assumptions.
Unknown support costs remain unknown and exclude that candidate. Estimates are
not charged amounts or proof that supporting model attempts happened.

There are 54 planned selector attempts: nine tasks × three strategies × two
phases. Errors and mismatched expectations remain in their partition/strategy
denominators. An expected stop is an assertion success, not a successful coding
task. `actual_model_attempts` is empty, and actual task outcomes, comparison model
costs and model latency are explicitly not-run/null. Exactly zero is reported
only for the harness's own model-call count and spend, since it makes no calls.

`selector_wall_ns` and its p50/p95 summaries measure the local selector call.
Synthetic evidence p95 values are inputs to ordering; they are never reported as
measured model latency. The small sample count and cold/cache effects do not
establish a production performance envelope.

The required decision-assertion rate is 100%; lower quality floors or changed
expectations cannot repair a failed run. Remote Jev-through-OpenRouter and
conventional-OpenRouter comparisons are explicit **not-run** rows awaiting both
separately authorized spend and qualified endpoint/protocol/host transport.
They are not replaced with an emulator. P6 live quality/cost/latency defaults and
P8 actual-delegation/packaged-CLI checks remain separate evidence gates.
