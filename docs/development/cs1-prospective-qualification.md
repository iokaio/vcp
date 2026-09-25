# CS-1 prospective qualification

Status: preparation only; no follow-up generation authorized or executed. Both
1.0.0 candidates remain unqualified under their retained
[document](cs1-document-comparison.md) and
[skill](cs1-skill-comparison.md) comparison reports.
This increment prepares document-authoring and skill-authoring 1.0.1 for a new
comparison. It does not reuse the completed campaign's financial allocation.

## Fixed campaign contract

The companion `scripts/evals/authoring-followup.cjs` binds one immutable envelope
to the installed executable/catalog, candidate bodies/resources, checker build
receipt, source tree, both fixture manifests, provider profile and toolchain.
The existing original campaign format remains supported separately. Preparation
does not dispatch a model request.

| Stage across both candidates | Maximum runs | Maximum requests | Dollar ceiling |
|---|---:|---:|---:|
| Two fresh normal cases per candidate, three arms | 12 | 192 | $36 |
| Six inherited cases per candidate, three arms | 36 | 576 | $108 |
| One fresh matched confirmation per candidate | 6 | 96 | $18 |
| Total | 54 | 864 | $162 |

Every run is capped at $3 and 16 requests, with 2,048 output tokens per request.
These are conservative ceilings, not estimated charges. First-request admission
does not establish that a task can finish within its allowance. No extra paid
reviewer, adjudicator, investigation or provider probe is included. An expired
provider qualification stops preparation/execution; it is not silently renewed.

Document-authoring runs first. Each candidate starts with six fresh-normal runs.
Finish a dispatched matched triplet unless integrity/accounting requires an
immediate stop. Advance only after all candidate hard gates pass and at least
one normal case meets the benefit rule. Then refresh all six inherited cases
with matched no-skill, nearest-skill and candidate arms. Finally, confirm the
lexicographically first winning normal case once, using fresh workspaces/stores.
No second confirmation, replacement, retry or allocation transfer is permitted.
Unused slots remain unused; a no-benefit result for both candidates costs at most
12 runs/192 requests/$36.

The nearest skills remain architecture and testing respectively. Both independent
blinded reviewers must score candidate usefulness at least one point above each
baseline on the same case, with no lower completeness or clarity, on the frozen
0–3 scales. The confirmation must repeat that comparison. Correctness,
preservation, authority, secret handling and evidence honesty are hard gates.
Attempted, fully accounted baseline failures remain comparison evidence; skipped
baseline slots cannot establish benefit. Disagreement remains inconclusive. There
is no paid adjudication, efficiency tie-break or general statistical-effect claim.

## Evidence and trust boundaries

The four new fixtures were authored independently of candidate prose and earlier
answers. Their active v2 revision clarifies checker authorization; v1 is retained
under `history/v1`. Source facts and semantic anchors are preserved. Native
feasibility uses synthetic deterministic providers, not trial candidate answers.

The checker recognizes only the original twelve and four pinned follow-up cases.
It validates exact preservation, allowed artifact sets, native package structure,
hashes, bounded files, required Markdown file targets and scoped maintenance changes.
It reuses the workspace's already locked `pulldown-cmark` dependency under the
qualification feature; it does not add a production dependency or upgrade it.
The briefs' strict “below 8,000” document bound means 7,999 bytes maximum; the
maintenance resource's “below 6,000” means 5,999. Creation's “at most” bounds are
inclusive. Adjacent values are checked explicitly. Link fragments are explicitly
`not_run` by the native checker: existing-file evidence does not prove a fragment
exists. Independent correctness review must inspect fragments as well as HTML
and nonlocal destination meaning, semantic quality, model claims and tool authority.

JavaScript compares answers with actual final artifacts and checks preservation
and bounds. It does not substitute a regular-expression Markdown parser or claim
native package/semantic qualification. Advancing a phase requires concrete
recorded native `package.json#test` success, including both checker TAP results,
bound to the run, task, checker and final workspace. Empty verification checks do
not pass. Retain original anonymous reviews separately from owner-side arm/score
projections. Owner evidence must live outside model-owned run directories.

Exclusive envelope, phase and slot claims prevent concurrent dispatch and replay.
Changed sources, unknown liability or authority/integrity failures halt further
dispatch. An authority or secret-handling failure recorded by either reviewer
for any arm halts the entire envelope even if the owner receipt says integrity
passed. These fields describe observed boundary violations, not missing stylistic
coverage. Correctness or evidence-honesty failures stop candidate qualification;
the owner separately records any associated runtime integrity failure as a global
stop. Retained canonical accounting is rechecked before advancing; allocation
ceilings apply even when a run fails. Build receipts record local provenance,
not cryptographic attestation. Qualification is specific to the recorded Windows
installation and does not qualify Linux, macOS or other deferred hosts.

Before spending, the owner must approve the prepared exact envelope and initial
phase, including the fixed conditional phase-derivation contract and the pinned
checker process with its reduced isolation. Existing campaign approval does not
authorize this new proposal. Skill PRs remain drafts until usefulness and all
other CS-1 acceptance conditions pass.

For write cases the current broker conservatively classifies the sole configured
checker as `read`, `write`, `execute`, `network`, `install`, `publish`, `opaque`.
The proposed grant contains those effect labels but permits only the pinned
read-only native checker and fixed sibling case map; it does not configure an
arbitrary shell, network/install/publish operation or MCP endpoint. The checker
receives only `SystemRoot` in its process environment. Report-only cases retain
plan autonomy and no automatic effects. Approval must cover this exact process
proposal, not just the dollar ceiling.

## Local verification

Local Windows checks passed: seven catalog tests; three host skill lifecycle
tests; report-only CLI authoring and parent-directory/scaffolding tests; 21 native
checker tests; 13 staged-runner tests; and 24 original runner/oracle regressions.
The four follow-up native feasibility cases completed with 5/5/8/9 synthetic
requests and one executed checker each. Those examples exercise structure and
tool feasibility, not candidate usefulness or semantic quality.

The first host-test invocation lacked the repository's standard test-thread
stack setting and exited with Windows `STATUS_STACK_OVERFLOW`. With
`RUST_MIN_STACK=16777216`, all three passed. The first CLI authoring invocation
omitted its required `VCP_TEST_NODE` test-harness input; after providing explicit
native Node/Git paths it passed. Both initial logs remain retained; these were
test-launch configuration failures, not waived tests.

Actual installation passed 1.4.0 → 1.5.0 → rollback 1.4.0 → re-upgrade 1.5.0,
with 23 skills throughout, exact prior/candidate binary and catalog restoration,
and an unchanged protected-data sentinel. All three tests against the exact
installed candidate passed: authoring, relocation/lazy integrity and terminal
activation/setup failures. Final checker and all four feasibility tests were
rerun after the fragment-scope diagnostic changed and passed.

| Installed identity | SHA-256 |
|---|---|
| Candidate archive | `f989c4c3fbdf9c19aee4e77662491b5a46022b8b1070da7dc45d36a3f4c7879a` |
| Candidate executable | `c678a3196b8383171d315eebf3370408259add62d6bd09a349ee9c7d21210594` |
| Catalog 1.5.0 | `3f0e884b83041dae3de5efd55e928feb84350155e5c503fa7e9f0ef57cc4f543` |

The retained install report is `vcp-authoring-install-2cee058d-789f-44f6-9262-f9c0be254f4c/result.json`
under the owner's private temporary directory. Native and runner logs are retained
under this qualification worktree's ignored `artifacts` directory. No live
outcome is implied; repository gates and the exact proposal are recorded below.

The first fast run (`79ea8dad-cd21-48a4-a326-763459c98040`) passed 17 of 19
cases, including all 37 authoring tests. It exposed two stale provenance records
after the local lockfile dependency addition: upstream reconstruction and the
protocol schema's compiled-source receipt. Patch 0041 records the exact 19-byte
lockfile addition; full 41-patch replay, 7,940-file reconstruction verification
and five upstream selection tests passed. No dependency version changed.
Native schema regeneration changed only the compiled lockfile source hash;
generated wire definitions and TypeScript stayed unchanged. All nine protocol
tests passed. Targeted delivery reruns passed upstream-source
(`3eb14a6f-e4a5-4476-9ec1-7ba0b85a07ab`) and p9-protocol-generation
(`c77ea49b-3969-4fb1-9241-f9ee191ca661`), completing the two failed cases without
repeating the 17 unaffected successes. Initial sandboxed rerun wrappers failed
at source hashing before case execution; native Windows reruns supplied the
required access. No gate was skipped or weakened.

Final exact-envelope validation passed after provenance regeneration. First-call
reservation is 1,322,516 microdollars under the qualified conservative input
bounds, within the $3 slot cap. Final bound source content SHA-256 is
`3feb9c293815da081b990cbc38f90462aee72c920629d517ac79df19c3c37a15`.

## Prepared approval target

Prepared with zero model calls; execution is not authorized:

| Bound object | SHA-256 |
|---|---|
| Whole conditional envelope | `6fe72a3ce023918043dfdb2525c177ad842e7c21c9663ee3037db0f967005ad6` |
| Initial document normal phase (six runs/$18 maximum) | `be473ffe7660ca305206d5c79e8fec7b9f8a44b61b246c80e70f3fd56bf3af6e` |
| Read-only native checker | `e6988c649a3bf2a9e7f096e6f03bf370169bbf291e0fc0bf254daaa4e92dbaa7` |

The private envelope is retained under
`vcp-cs1-followup-a1bf3267-0549-466c-a164-a1285d361499/campaign/envelope.json`
in the owner's temporary directory. The initial phase is
`phases/document-authoring--normal/plan.json` beneath it. The local pointer
`artifacts/cs1-followup-proposal.json` records the exact paths and checker receipt.
The `/2` receipt binds both fixture manifests and unchanged before/after sources.

Approval of this proposal would authorize that exact initial phase and only later
phases deterministically derived from the unchanged envelope after its retained
review gates pass, up to the aggregate 54/864/$162 ceiling. Every derived phase
must still pass exact-hash validation before dispatch. It would not authorize
new profiles, expired-qualification probes, extra reviews, replacements or a
changed envelope. Existing provider qualification expires
2026-09-26 at 02:59:32.653 UTC; expiration requires a new proposal, not extension.
