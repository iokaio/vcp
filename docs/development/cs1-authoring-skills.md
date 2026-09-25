# CS-1 document-authoring candidate implementation

This page records the isolated 22-skill document-authoring increment. The separate
[skill-authoring increment](cs1-skill-authoring.md) adds its package on top; the
artifact identities and results below continue to describe the document-only build.

Owning item: [CS-1](../plan/24-skills-follow-on.md#cs-1--authoring-foundation).
Status: **accepted by owner direction** as an explicitly selected, non-default
candidate on September 25, 2026. Qualification failures and unrun evaluations remain
recorded; acceptance does not claim default qualification or authorize more paid calls.
Additional work is listed in the [candidates README](../../src/skills/candidates/README.md).

## Candidate boundary

The exact original document-authoring 1.0.1 body and descriptor live at
[the candidate source](../../src/skills/candidates/README.md).
There is no automatic candidate loader. The distributed builtin catalog remains
version 1.2.0 with 21 skills and 44 files; document-authoring is absent from its
inventory and coverage. An experiment must explicitly register a candidate source
through the existing source registry and select the skill. Ordinary Markdown
presence does not trigger activation, and a skill grants no tool authority.

The 1.0.1 package retains its frozen bytes, including the guidance whose interaction
with report-only tool ceilings was investigated after the mandatory halt. The P2-05
host tool-ceiling correction does not rewrite those bytes, excuse the violation or
qualify a new runtime/package combination. No frozen fixture, profile, campaign or
receipt is changed by the source-only disposition.

The descriptor requires read/list tools. Guidance covers source-backed project
documents, historical decisions, source links and reader review. Exact SHA-256:

- Body: e5c04dfb0eeb14f1e52868e217701b1ef07f37d634eae8497cafeec31d132d1f.
- Descriptor: 463b19cc77673316ccc7a55227d529c5746159b0173364b83b47967cebe1089f.

Skill-authoring is owned by its separate candidate increment. Shared
[frozen authoring fixtures](../../src/evals/skills/authoring/README.md) and oracle
checks remain evaluation data, not an installed skill. The P7 42-case inventory
is unchanged.

Document-specific native tests register the candidate explicitly and preserve
bounded loading, missing-tool rejection, changed/missing-body rejection, revocation
and workspace precedence. They also assert default builtin discovery cannot resolve
it. The CLI report-only test loads the exact candidate through an explicit user
source and checks provider context and unchanged workspace bytes. Asset staging
checks prove the candidate is excluded. These implementation checks do not establish
live usefulness or qualification.

## Closed 1.0.1 diagnostic outcome

The fresh normal comparison retained an implementation-source attribution error
and no passing usefulness benefit. The inherited hostile-content and near-miss
comparisons retained candidate factual failures. A no-skill report-only arm called
disallowed vcp_verify, triggering the mandatory all-arm halt. The closed campaign
dispatched 19 of 54 planned slots; later comparisons, including all new SKL v3
comparisons, did not run. No qualification transfers from another arm or version.

Cumulative retained provider accounting is 181 requests and $0.401397 with zero
active or unresolved liabilities. These are recorded campaign costs, not an invoice
or accounting for coordinator/reviewer AI use. Two independent blinded AI readers
provided reviews; they were not human readers. The final detailed outcome and
identities are retained in [the qualification evidence PR](https://github.com/iokaio/vcp/pull/175).

## Historical mixed-catalog campaign

CS-1 evaluation staging used both candidate packages: **23 skills at development
catalog 1.3.0**. That artifact is distinct from the current **21-skill** default catalog and explicit candidate source.
Its executable, catalog, source, fixture, profile, checker and proposal identities
must retain their original hashes. Do not relabel its package qualification as
qualification of this split artifact.

The completed mixed-catalog proposal is
`f6b86aa2323df01c39d69b2b4fb70a4214e419b47c2fdd4a97d2e87647fb48a3`.
All thirty-six runs have finished; the eighteen DOC runs have
[retained outcomes and accounting](cs1-document-comparison.md). Two independent
blind reviewers found the document
candidate and no-skill outputs tied overall on both normal tasks. The candidate
outperformed the architecture baseline, but that alone does not establish the
required benefit over both baselines. Recorded cost/request differences are
descriptive observations, not a retrospectively selected acceptance rule.

All three ADR-boundary runs failed native completion. The candidate preserved the
historical sources and represented their statuses correctly but omitted the links
required by the frozen oracle. Diagnostic execution of the same read-only checker
confirmed the missing-link failure. The P2-06 correction merged in PR #174 exposes
bounded retained process diagnostics to the model after verification. The frozen
campaign used the prior runtime; this does not erase its output failure or prove
that better feedback would have corrected it. Earlier failures and accounting
remain retained. Default promotion is blocked by these unmet gates.

## Historical document-only 1.0.0 checks

The following receipts describe document-authoring 1.0.0 and catalog 1.3.0, not the
current 1.0.1 candidate. Preparation checks passed: 12 focused JavaScript asset/oracle
tests and `git diff --check`. Asset verification observed 22 skills and 46 files; historical
catalog SHA-256 is
`d105557fe05cc6f5853b5c81bf48690a4887b5d79ac4dcdba6d0c2c5289b4bac`.
Seven native catalog tests, three host skill tests and the native document
report-only test passed in this worktree. The initial host invocation omitted its
required qualification feature and failed to compile fixture imports; the corrected
invocation passed. The nineteen-case fast suite passed in run
`d74ccfbc-039a-448b-8544-5d2157769dd8`.

The exact 22-skill package passed install, upgrade, exact rollback and re-upgrade,
preserving the synthetic protected-data sentinel throughout. Its installed binary
also passed document activation, relocated discovery/integrity and terminal skill
control tests. Archive SHA-256:
`c7892b82ca500e96d519763eb6303bd034e529ba1c69ac6bbeca89157641211e`.
Executable SHA-256:
`b10d6f6fa105f5f7fbf5ccf60a2761b565b1577873644e9386747dda5aee75a8`.
These are local debug qualification checks, not release or performance acceptance.


## Remaining acceptance gates

The failed correctness, authority and inconclusive benefit gates remain unmet.
The [follow-up design](cs1-follow-up-design.md) preserves the separate qualification
contract; any changed package or runtime requires appropriate fresh evidence.
The owner-approved source-only merge records an unqualified candidate and does not
waive these gates. Default distribution and CS-1 completion remain deferred until
both candidates meet their acceptance conditions.
