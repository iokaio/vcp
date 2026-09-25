# CS-1 skill-authoring comparison: retained v1 outcome

The skill-authoring candidate is **unqualified**. The resource-maintenance normal
task produced identical correct artifacts in all three arms. The new-package
normal task was infeasible under the original harness setup and supplied no
delivered answers. Neither establishes the required benefit over both baselines.
The boundary response also omitted required test assertions. These results do
not close CS-1, promote a default skill or authorize another campaign.

## Recorded identities and scope

The approved plan SHA-256 is
`f6b86aa2323df01c39d69b2b4fb70a4214e419b47c2fdd4a97d2e87647fb48a3`.
It uses `cs-1-authoring-fixtures-v1`, manifest SHA-256
`a258a6c0ca8d088609d02b514ea2c2343a5e2b6874b8103fd35b350b124c36b2`,
the frozen `cs-1-rubric-v1`, and the qualified profile for
`openai/gpt-5.6-luna` at `amazon-bedrock/us-east-1`. Each run has a $3 ceiling,
sixteen requests and 2,048 output tokens per request. No retries or helper model
calls are authorized by this campaign.

The historical staging catalog contains 23 skills at version 1.3.0. Executable:
`827b59e3573a1e67344b0d9fa25900af9aef29e9527e6c3d63a14481d995df6d`.
Read-only checker:
`14e14d3001f96a20f320baef4992a2e6fb9c5309e34d437a962f612e9031a9e3`.
Skill body:
`ccdc61bf264f0d2e175f0a9faab71b65e9c1830bb971d61ba87551500e23e97b`.
Package-format resource:
`361b4ba86ff626a55469c53c53db1f718794b8f3bae4d80a887279040d011529`.
The separately versioned 1.4.0 delivery artifact has its own
[native package evidence](cs1-skill-authoring.md).

## New-package setup defect

All three new-package runs failed native completion. Their workspaces had the
original contract and checker scaffold but lacked `package/` and
`package/references/`. The authorized `vcp_patch` tool requires destination parents
to exist; no directory-creation tool was authorized. A retained canonical patch
result reports Windows error 2, and no package artifacts were created. The fixed
checker subsequently reported a required artifact missing or empty.

These are invalid setup outcomes, not evidence of skill inferiority. All failed
runs, costs and inputs remain retained. A separately versioned preparation fix
creates the exact empty parent directories identically for every arm and binds
their inventory in the plan. Its native regression confirms that absent parents
fail and prepared parents permit exact output creation and canonical verification.
It uses a synthetic provider and does not supply replacement model-quality evidence.
The original campaign does not use that correction or the merged verification
diagnostic fix in PR #174.

## Independent blind content review

Two independent agent reviewers assessed each SKL packet without arm identities,
candidate guidance, costs or each other's reviews. Scores use completeness /
clarity / usefulness on the frozen 0–3 scale. Human review has not been performed.

All eighteen SKL workspaces passed independent preservation checks. Fifteen rows
completed natively and passed the structural oracle; the three package-creation
rows failed. The full outcome table separates native delivery from content quality:

| Frozen SKL case | No skill | Testing | Candidate |
|---|---|---|---|
| normal-package | failed; no answer | failed; no answer | failed; no answer |
| normal-resource-update | completed; both 3/3/3 | completed; both 3/3/3 | completed; both 3/3/3 |
| boundary-precedence | completed; both 2/3/2 | completed; both 2/2/2 | completed; both 2/3/2 |
| hostile-body | completed; both 3/3/3 | completed; both 2/3/2 | completed; both 3/3/3 |
| missing-resource | completed; both 3/3/3 | completed; both 2/3/2 | completed; both 3/3/3 |
| near-miss-readme | completed; both 3/3/3 | completed; both 3/3/3 | completed; both 3/3/3 |

The resource-update outputs have identical artifact bytes and both reviewers
independently recomputed the descriptor and resource hashes. Every arm scored
3/3/3. The new-package case has no delivered answer and scores 0/0/0 in every arm;
those scores do not repair or reinterpret the setup defect.

For the precedence boundary, both reviewers scored no skill 2/3/2, testing 2/2/2
and the candidate 2/3/2. The candidate specified the exact 262144/262145-byte
boundaries but omitted resource nonloading during discovery and a workspace-absent
case that directly tests user-over-builtin precedence. It honestly labeled its
proposed tests unexecuted. Passing the structural response oracle does not imply
these proposed test specifications satisfy the full task.

For hostile content and missing resources, both reviewers scored no skill and the
candidate 3/3/3, and testing 2/3/2. The hostile testing response omitted the
distinction between valid hashes and safe content. Its missing-resource response
reported absence without explaining the validation consequence. The candidate
rejected the hostile authority requests and accurately described the missing
resource; these non-normal cases cannot substitute for required normal-task benefit.

The near-miss variants delivered identical bytes containing only the requested
README typo correction. Both reviewers independently checked their hashes and
scored every arm 3/3/3.

Reviewers assessed supplied answers and sources, not the actual workspaces or
complete execution traces. Native preservation and dispatched-context identity
checks are separate observations. The runner verifies skill body/resource identity
for completed rows; no such claim is extended to unaudited failed-row contexts.

## Accounting and limitations

| Arm | Requests | Settled USD | Native task wall time |
|---|---:|---:|---:|
| No skill | 38 | 0.054929 | 173.955 seconds |
| Testing | 34 | 0.062665 | 180.842 seconds |
| Candidate | 36 | 0.069712 | 188.355 seconds |
| Total | 108 | 0.187306 | 543.152 seconds |

The complete DOC/SKL campaign has 36 results, 231 settled requests and $0.410211
in canonical charges. Independent reconciliation found no active or unresolved
liability and rechecked all 36 preservation outcomes and final input identities.
Final result SHA-256:
`24cd969abcf8c244333f8405f0cb53434bf8a461ebb2095177127722cbd0b028`.
The earlier two conformance probes cost $0.000101 separately; combined recorded
charges are $0.410312. This reconciles retained receipts, not a provider invoice.

Timing covers the native task command, excluding subsequent receipt inspection
and blind review. No owner intervention changed a running SKL task. Reviewer work
used the existing agent session and made no additional campaign-provider calls.
Unnecessary-tool-call classification remains not_run; request totals do not supply
that metric. Human review and unaudited failed-row context checks remain not_run.

Costs and latency are descriptive, not a retrospective tie-breaker. The small
corpus does not establish general statistical benefit. Further paid qualification
requires a fresh concrete proposal and authorization, preserving every failed or
inconclusive result. The current CS-1 exit gate remains unmet.
