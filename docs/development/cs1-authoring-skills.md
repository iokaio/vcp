# CS-1 document-authoring delivery preparation

This page records the isolated 22-skill document-authoring increment. The separate
[skill-authoring increment](cs1-skill-authoring.md) adds its package on top; the
artifact identities and results below continue to describe the document-only build.

Owning item: [CS-1](../plan/24-skills-follow-on.md#cs-1--authoring-foundation).
Status: refined document-only increment prepared for draft review; current native
package checks and live qualification remain outstanding. This document does not
authorize promotion, release or additional paid calls.

## Proposed inventory

The proposed catalog is version 1.3.1 with 22 skills: the existing 21 families and
original `document-authoring` 1.0.1. Its descriptor requires only read/list tools,
uses the explicit cue convention and grants no execution authority. Guidance covers
source-backed project documents, preserved historical decisions, source links and
reader review. Ordinary Markdown presence does not trigger activation.

The 1.0.1 refinement captures explicit citation and output requirements and checks
them before delivery. Its body and descriptor match the frozen refined candidate;
this document-only inventory has its own identity. Coverage remains
`provided_unqualified`; neither package integrity nor historical results establish
qualification of the changed body.

`skill-authoring` is absent from this increment. It belongs in a separate PR with
its resource and catalog/native tests if its own qualification succeeds. The
two-candidate campaign preparer/runner and future candidate packages are excluded.

## Shared evidence inputs

The [frozen authoring fixtures](../../src/evals/skills/authoring/README.md),
`scripts/evals/authoring-oracle.cjs` and its contract tests retain the original
twelve-task specification unchanged. Their SKL fixture content is evaluation data,
not an installed skill. Independent oracle tests do not activate an absent skill
or qualify live usefulness. The original P7 42-case inventory remains unchanged.

Document-specific native catalog tests cover explicit activation, bounded loading,
missing tools, changed/missing body bytes, revocation and workspace precedence.
A native report-only test checks actual activation before the provider request and
preservation of workspace bytes. Inventory tests distinguish 22 builtin entries
from the terminal fixture's 23 entries including its workspace-owned override.

## Historical mixed-catalog campaign

CS-1 evaluation staging used both candidate packages: **23 skills at development
catalog 1.3.0**. That artifact is distinct from this proposed **22-skill** catalog.
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

## Current 1.0.1 preparation and remaining gates

The current 22-skill, 46-file inventory has catalog SHA-256
`e1575c1bf931b567ad07667b3339c54f1ac29f74744d3487a688e47e765488b1`.
All 12 focused JavaScript asset/oracle tests passed for this refresh.
Fresh native package, installation and live qualification evidence is required for
this changed artifact; the historical receipts above are retained without transfer.

Before merging this increment, resolve the failed correctness and inconclusive
benefit gates established by the completed document comparisons. The
[follow-up campaign](cs1-followup-outcome.md) halted on its frozen tool-authority
gate. The [prospective follow-up design](cs1-follow-up-design.md) records the
separate qualification path; its execution evidence belongs to its exact frozen
artifacts and cannot relabel historical results as passing. Any changed
package or runtime needs appropriate fresh qualification. CS-1 remains incomplete
until both separately delivered candidates satisfy their acceptance conditions.
