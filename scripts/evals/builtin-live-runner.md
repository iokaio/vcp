# P7-02 live skill usefulness observations

`builtin-live-runner.cjs` prepares sixteen matched, read-only CLI runs: baseline
and explicitly activated skill for each normal/negative fixture in architecture,
review/debug, testing and JavaScript/TypeScript. This is selected U01/U02/U08
guidance evidence. It does not establish U03 generation or exhaustive ecosystem
support. Remaining families and native toolchain results retain their own status.

Preparation makes no model calls. Supply a private spec containing only
`executable`, `profile` and `aggregate_cap_usd` (an exact USD decimal string).
The executable must have the exact current bundled assets beside it and support
`run --skill`. The fixed-provider profile must already be qualified, disable
retries, allow at most sixteen requests and 1,800 seconds per run, explicitly
request 1–8,192 output tokens within the qualified provider maximum, and use plan authority with no processes, executable checks,
MCP, routing, evaluator or custom skill sources. The existing credential
environment boundary is unchanged; embedded credentials are rejected.

These are P7-specific coding limits. The frozen P6 smoke runner retains its
eight-request, 600-second and 512-token limits. A fresh P7 trial should normally
use 4,096 output tokens, sixteen requests and 900 seconds: the previous 512-token
generation attempts truncated patch arguments. Changing these bounds, the
executable or any other bound input invalidates an old plan and requires a fresh
exact-plan authorization. Historical failures and their allocated liabilities
remain retained; unused allocations are not recycled.
The new campaign needs a separately approved exact-plan cap. All original
allocations and unresolved liabilities remain held; the new plan cannot replay
old attempts or fund itself from their unused allocations.

```powershell
node scripts/evals/builtin-live-runner.cjs prepare C:/private/spec.json C:/private/new-p7-trial
node scripts/evals/builtin-live-runner.cjs run C:/private/new-p7-trial/plan.json <authorized-plan-sha256>
```

The user must authorize the aggregate spend cap before execution. A prepared plan
hash binds the selected executable, package, fixture bytes, runner sources,
provider profile and catalog. The runner divides the cap equally among all sixteen
attempts without recycling unused allocations. Every run uses a fresh canonical
store and immutable starting fixture copy. A permanent claim prevents replay;
an interruption or unknown charge stops subsequent dispatch and requires
inspection. It does not imply a free retry.

Results retain exact command arguments, JSONL, canonical cost/routing/output/context
inspection pages and digest-checked final response bytes. Canonical context
manifests bind the expected active skill body to every settled request; baseline
manifests must contain no active skills. File preservation and canonical settled
costs are checked separately from answer quality. All failed/not-run cases remain
in the denominator. Missing final JSON answers are failures rather than discarded
samples.

The frozen fixture `behavior_rubric` is held outside the model workspace. An
independent reviewer grades each observed answer against that rubric and actual
fixture files, records supported findings, false positives, missing evidence and
honest not-run statements, and compares both arms. The runner deliberately leaves
`quality: pending_independent_review`; successful execution is not a usefulness
grade, owner sign-off or a claim of statistical superiority. Full U03 generation
requires separate executable feature assertions and preservation evidence.
