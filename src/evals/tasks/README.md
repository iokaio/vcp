# P6 task-quality preparation

`p6-task-quality-v1.json` freezes six original synthetic tasks: analysis, review,
and generation in each of the tuning and held-out partitions. File/prompt hashes
give all three strategies identical starting states. Fixed economical, fixed
stronger, and routed arms remain unbound to actual models and explicitly not run.

The fixture inputs and grader-only labels are separate files under
`../fixtures/p6-task-quality/`. Only the selected task's prompt and files belong
in a future model request. Do not send labels, probes, other cases, or the grading
implementation to the model. Analysis grades direct dependencies and citations;
review grades located defect categories; generation grades a small declarative
validation schema against independently authored boundary cases. Model output
is never executed as code. These narrow tasks are preparation and smoke coverage,
not a representative software-engineering benchmark or sufficient sample for
shipping profile qualification. Generation exercises schema generation rather
than arbitrary code editing or tool execution.

From the repository root:

```powershell
node scripts/evals/p6-task-quality.cjs prepare
node --test src/tests/contracts/p6-task-quality.test.cjs
node scripts/evals/p6-task-quality.cjs grade path/to/answers.json
```

Preparation emits 18 explicit not-run rows with matching start-state hashes and
no labels. Grading accepts a JSON document containing `revision`,
`manifest_sha256` from preparation, and `runs`. Each run contains exactly
`case_id`, `strategy`, `start_state_sha256`, `status`, and `answer`.
Status is `completed`, `failed`, `cancelled`, or `not_run`; `answer` is the parsed
JSON response, or null when unavailable. Missing rows remain unsuccessful in
the planned denominator. Duplicate/unknown runs and identity drift are errors.
The grader exits nonzero on invalid submission shape or frozen identity;
behavioral failures are reported in `grade.pass` and strategy/partition counts.

Grades are explicitly **unverified submitted answers**. Supplying good fixture
answers establishes grader correctness, never provider performance. Cost and
latency remain unknown even for passing answers. No cost claims are imported;
a future executor must join every main/helper/retry/compaction/optimizer/
verification/child attempt to canonical reservations and settlements, preserving
unresolved liabilities. Provider unavailable, failed and cancelled runs cannot
be dropped or silently replaced. Serious review misses and wrong boundary
behavior fail the fixed rubric.

The reusable manifest grants no trial authorization; its configured cap is null.
This grader imports no network client and
cannot dispatch trials. A real run first needs an authorized configured cap,
current model/provider/catalog/policy identities, qualified finite charge bounds,
canonical admission, and an execution/grading provenance record. Freeze expanded
independent task coverage, statistical sample requirements, execution order and
catalog drift handling before evaluating shipping defaults. Held-out results
cannot tune this revision. Actual Jev/conventional evaluator utility and P8
delegation/packaged-CLI qualification remain separate not-run gates.

`scripts/evals/p6-live-runner.cjs` prepares isolated CLI trials from externally
supplied trusted profiles and an explicit aggregate cap. It binds the executable,
profiles, catalogs, fixture and grader hashes before execution, and requires the
exact prepared-plan hash to run. Each trial gets a fresh workspace and canonical
store. A durable one-shot claim prevents replay after an interrupted invocation.
Canonical cost, routing and response inspections supply the execution evidence;
unknown liability stops subsequent dispatch. The runner never turns these six
cases into shipping qualification or invents missing model memberships.

The manifest's `executor: unimplemented` records its original preparation state;
the runner and its source hash are recorded in each later execution plan.
