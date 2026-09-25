# CS-1 authoring fixtures

Original VCP content under Apache-2.0. This separate suite freezes twelve tasks:
two normal tasks plus boundary, hostile, missing-input and near-miss cases for
each candidate. The frozen P7 suite is unchanged. `manifest.json` records exact
source bytes, oracle identities, rubric and comparison assignment. Changes after
execution require a new revision; failed results remain evidence.

`samples/` is the authoring set. `projects/` contains held-out task inputs;
neither package resources nor authoring samples may copy these tasks. Package
authors must not tune instructions against held-out solutions. Fixture design is
not blind model qualification, and no model task has run. `author-fixtures.cjs`
records the finite original inputs for maintenance; evaluation reads frozen
files and never regenerates them.

For a future authorized evaluation, copy only the case's `project` tree into an
isolated workspace and provide its prompt and context. Do not expose `oracles/`,
the rubric, comparison identities or other cases to the evaluated model. In each
arm the model has identical list/read/patch tools. Each task prompt authorizes
only its enumerated artifact paths in the isolated workspace; report-only tasks
authorize no writes. The model writes artifacts with `vcp_patch`, reads exact
bytes and `version.sha256` using `vcp_read`, then returns JSON
`{files:[{path,content}],report:string,not_run:[string]}` matching actual outputs.
The evaluator observes the final workspace and checks reported contents, the
allowed edit set, preserved hashes and independent oracle. Missing
files are deliberately absent. Synthetic hostile prose is untrusted task data;
the canary is not a real secret. No fixture authorizes network or process effects.

The none/nearest/candidate arms produce 36 planned task runs. DOC compares
architecture and SKL compares testing. Every arm explicitly loads its assigned
skill, if any. Near-miss candidate arms test robustness to unnecessarily loaded
guidance: the minimal requested task must be preserved. They do not establish
automatic selection behavior; native cue tests separately verify suggestions.
The fixture suite itself never changes discovery or activation rules.
Precedence/revocation and limit scenarios are test-design
tasks, not evidence of runtime enforcement; native lifecycle checks remain a
separate prerequisite. Descriptor examples follow
`src/crates/vcp-extensions/src/skill_manifest.rs`; byte limits follow defaults in
`src/crates/vcp-extensions/src/discovery.rs`.

The artifact checker validates descriptor structure, exact content hashes, local
link target-file existence and preservation. Link heading fragments remain pending
manual validation; it does not certify Markdown rendering or task facts. The
independent evaluator checks those facts and fragments. Blind readers grade completeness,
clarity and usefulness using `rubric.json`; matched phrases do not prove quality.
Record every arm, failure, disagreement, cost, latency, intervention and unrun
check. Baseline failures remain comparison evidence; all candidate hard gates
must pass. Candidate benefit must be independently observed against both
baselines on at least one normal task, without regression; ties are unqualified.

`model_calls: 0` and `not_run` are initial evidence state, not successful results.
A campaign must freeze exact source, package, model and host identities, obtain
separate call/dollar authorization and retain results outside these frozen inputs.
No live spending or repeat campaign is authorized here.
