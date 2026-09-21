# P6-04 measured profile comparison

The September 21, 2026 campaign rejects automatic profile-default activation.
It does not replace explicitly configured model choices or lower the routing
quality floor. This is a measured negative subsystem outcome, not a shipping
model ranking or a claim that either model cannot solve ordinary coding tasks.

## Frozen protocol and tuning decision

[Task corpus v3](../../src/evals/tasks/p6-task-quality-v3.json) contains 18
synthetic analysis, review and schema-generation cases: three tuning and three
held-out cases per class. The three declared strategies retain 54 planned rows.
The [activation gate](p6-profile-gate.md), fixtures, grader, executable, provider
catalogs and profile identities are hashed before execution. No model substitution,
human repair or retry is permitted. All failed and unavailable rows remain in
their planned denominators.

A separate tuning bootstrap ran both fixed models on all nine tuning tasks.
Economical passed 1/9 (1111 bps); stronger passed 0/9. Both fail the predeclared
8000 bps experimental routing floor. Consequently, the final comparison keeps
both fixed baselines eligible for independent measurement and records every
routed row as not-run with no eligible candidate. Neither fixed model's poor
tuning score silently removes its baseline. No held-out answer changes routing
membership, ordering or thresholds.

The 48 bootstrap provider requests cost **$0.095858**. Each trial used a fresh
workspace and canonical store. Its source-bound result is
`5aa639d8d9ba67fa23bab4bc0ffc00abffa4fd03a371f748a4afc2ff1f07ee94`.
The final comparison plan is
`5ad8b94a780802bbb8510c01c1eb78bf56d2743614dafbb9a1b5ccfe69c6517b`.

## Final matched comparison

The [reviewed result](p6-profile-comparison.json) retains all **54 planned rows**:
36 attempted fixed-provider tasks and 18 routed not-run tasks. The 93 observed
provider attempts cost **$0.188138**, including failed tasks. Economical uses
`anthropic/claude-3-haiku` at `amazon-bedrock`; stronger uses
`anthropic/claude-haiku-4.5` at `anthropic`. Each starts from the same frozen case
files in an isolated workspace and receives the same four-request and 512-output-
token bounds. There were no human interventions, retries, model substitutions or
remote grader calls.

| Strategy | Partition | Passed / planned | Observed task spend | CLI p50 | CLI p95 |
|---|---|---:|---:|---:|---:|
| Fixed economical | Tuning | 1 / 9 | $0.020031 | 10.545 s | 13.912 s |
| Fixed economical | Held out | 0 / 9 | $0.025767 | 11.681 s | 15.455 s |
| Fixed stronger | Tuning | 0 / 9 | $0.069005 | 7.456 s | 10.021 s |
| Fixed stronger | Held out | 0 / 9 | $0.073335 | 8.523 s | 9.881 s |
| Routed | Tuning | 0 / 9, all not-run | Not observed | Not observed | Not observed |
| Routed | Held out | 0 / 9, all not-run | Not observed | Not observed | Not observed |

Each measured latency distribution includes all nine attempted tasks, including
failures, and measures CLI execution rather than subsequent evidence-inspection
time. With nine observations, nearest-rank p95 is simply the maximum. These
numbers do not establish a population tail-latency claim. Each fixed model passes
zero held-out tasks in every class; all class-level one-sided success lower
bounds are therefore zero.

The sole passing final-campaign task is economical `tuning-generation`. Thirteen
economical tasks and all eighteen stronger tasks fail canonical final-answer
import; the other four economical tasks fail the bounded host task. The strict
output protocol is part of acceptance. No answer is repaired or extracted from
prose to improve reported success. The publication retains status and classified
import reasons, so low scores cannot be mistaken for a separately measured count
of semantic defects.

Final result SHA-256:
`332255559c6787c7b3fe093eb6b3b5a97864171edaa8216808c41e05c8d90657`.
The JSON records executable, manifest, gate, grader and reporter hashes plus the
hashes of every inspected canonical cost-page set. Root-only main attempts account
for these trials; helper, child, remote verification and evaluator categories were
not invoked. The valid tuning bootstrap and final comparison together cost
**$0.283996**. The separate diagnostic cost and earlier unresolved provider
liabilities remain in the overall P6 ledger; this report neither drops them nor
claims that $0.283996 is the whole milestone's spend.

## Failure interpretation and limitations

The initial v1 diagnostic revealed two harness issues: read-only completion
needed an existing owner-authorized integrity proof, and captured responses were
incorrectly rejected for privacy omissions that excluded authentication headers
and recovery material before byte capture. Those issues were fixed before v3.
The diagnostic's $0.079433 remains charged and its results remain ineligible for
profile qualification. Read-only reimport preserved the original verdicts and
confirmed captured body hashes without replaying requests.

V3 also resolves an instruction ambiguity discovered in that diagnostic: every
prompt explicitly requires exactly one JSON object, with no prose, Markdown or
XML wrappers. Substantive source files and grading probes remain unchanged; v2
was never executed. Actual v3 requests retain that instruction. Inspected host
system instructions require no conflicting output wrapper. Answers containing
correct JSON inside explanatory prose still violate the frozen serialization
contract and fail; the grader does not strip wrappers after seeing results.

This measures the entire bounded CLI task, including tool use, request-count
limits and strict final-answer compliance. It does not isolate general reasoning
quality from host interaction or formatting compliance. No routed cost, success
or latency is observed when no candidate meets its quality floor. Those values
remain null; not-run rows cannot become zero-cost successes.

Even an all-pass result on this corpus would fail the separate population
activation gate: three synthetic held-out cases per class do not meet its
30-independent-task minimum or establish representative coverage. Reported
one-sided exact binomial bounds and nearest-rank p50/p95 are descriptive;
repeated tuning executions do not increase independent sample counts.

[P5-08 memory evidence](p5-08-integrated-memory.md) remains the applicable
retrieval/retention prerequisite. The campaign proposes no changed memory or
context default. P8 retains actual delegation, packaged CLI and final integration
qualification. Provider/catalog/prompt drift, serious failures or unresolved
charges invalidate any later activation rather than weakening the gate.

## Public evidence boundary

The raw reporting command emits private profile and catalog paths under its
`strategies` field; its output stays private. Reviewed publication uses a field
allowlist: immutable hashes, public model/endpoint identities, synthetic case IDs,
statuses, rubric verdicts, cost and latency counters. It excludes local paths,
raw response bodies, account-linked request IDs and canonical owner scopes.
Original canonical records remain local for audit. Hash binding detects identity
drift; it is not a substitute for the canonical admission/settlement audit.
