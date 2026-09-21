# 22 — Cursor-derived improvements in the remaining work

Status: selected planning refinements; implementation remains pending for the
deltas below. This document adapts [the Cursor research](../research/cursor.md)
to the current VCP baseline, inspected at `da14cc3` on September 21, 2026. It
does not implement features or reopen completed acceptance. The
[task ledger](20-traceability.md#work-item-ownership-and-readiness) and linked
owning sections remain the implementation entry points.

The useful near-term additions belong to P7-02 and P7-04 through P7-06, which
are **in progress**, followed by the planned P8 qualification campaigns.
Foundational P2/P3/P5/P6 services and completed P7-01/P7-03 are dependencies to
reuse and regression-test, not new primary work items. The 68 task IDs,
dependencies and 56-task P8-05 closure remain unchanged. Selected deltas become
part of their open owners' remaining acceptance; deferred ideas are outside the
current release commitment.

The research is input, not an adoption checklist. Architecture draft 0.4 and the
ADR register take precedence. Cursor is closed source: this plan adopts useful
patterns without copying code, prompts, documentation or proprietary defaults.
[The Claude Code supplement](23-claudecode-improvements.md) folds overlapping
ideas into the same owners and fixtures rather than adding parallel mechanisms.

## Selection and developer experience

Prefer improvements that help developers find relevant code, run realistic
checks, understand a child task and review its result. Keep normal work within
the existing skills, `/agents`, status, evidence inspection and pause/resume
flows. A simple task can remain single-agent. Do not require a new project
configuration file, background service, mode or confirmation for an already
authorized operation.

| Research increment | Disposition | Remaining owner and reason |
|---|---|---|
| CR-02a Bounded regex search | Selected, with CC-04 source ranges and path filtering | P7-02: useful analysis and review should reach a relevant region without repeated whole-file reads or shell searches |
| CR-03 Helper roles | Selected: small `explore` and `review` templates | P7-04/P7-06: reuse read-only children and make routine delegation usable; no new authority or automatic model switch |
| CR-08 Worktree setup and cleanup | Selected: readiness diagnostics and deliberate cleanup | P7-04/P7-05/P7-06: finish existing ownership and cleanup obligations; automatic setup recipes and periodic eviction are deferred |
| CR-12a Local change review | Selected: current-diff findings and clear scope | P7-05, with P7-02 guidance: improve the existing review path; no patch-only cache, review database or new rule-file hierarchy |
| CR-06 Debug workflow | Selected: bounded evidence and cleanup guidance | P7-02: extend the packaged `review-debug` procedure; no instrumentation store, completion-state extension or rewind dependency |
| CR-10a Protected operations | Selected as qualification of existing boundaries, with CC-08 | P8-02: exercise existing policy and child enforcement; do not introduce blanket deletion approvals or new policy defaults |
| CR-13 Dynamic context discovery | Reuse and qualify existing behavior | P8-03: full-output artifacts, compaction provenance and MCP diagnostics already have owners; new model-facing history tools and static server-context parts are deferred |
| CR-15 Post-edit diagnostics | Folded into existing M6 and P7-05 verification | Use available project checks against the current parent; no LSP or shadow-workspace subsystem |
| CR-01 Whole-workspace semantic index; CR-02b indexed regex; CR-14 retrieval fitting | Deferred | Existing local retrieval and the planned M5 comparisons come first; no measured need yet for another index, publisher or learned ranking path |
| CR-04 File rewind | Deferred | Valuable recovery candidate, but durable before-image retention, rename semantics and pruning need a separately scoped future increment; session fork is not file undo |
| CR-05 Durable plan artifact | Deferred | Existing plan autonomy, task acceptance and attributed input cover current work; a new plan lifecycle and command family are unnecessary for the open P7/P8 gates |
| CR-07 Best-of-N | Deferred | Multiplies spend and worktree lifecycle cost; it does not by itself supply held-out evidence that qualifies P6 advice |
| CR-09 Scoped fragments | Reuse existing instructions/skills; foreign import stays with P10-02 | Nested `AGENTS.md` and lazy skills already cover common needs; new activation semantics need concrete evidence of a gap |
| CR-11 Queue, side sessions and long-lived goals | Deferred | Preserve bare-input steering, explicit pause/resume and bounded tasks; do not add competing input meanings or a second task lifecycle |
| CR-12b Learned review rules | Deferred | Current review usefulness and false findings must be measured before adding a feedback store or learning mechanism |
| CR-16 Agent Client Protocol | Evaluate within P9-01 when it starts | Compare compatibility with VCP's public contract then; no first-release dependency |
| CR-10b Model approval triage; hidden routing; hosted/shared index; cloud agents; team/marketplace features; editor/browser features | Not selected | Outside the selected developer workflow or inconsistent with VCP's authority, local-compute, transparency and CLI-first boundaries |

These dispositions supersede the broader proposed waves in the earlier version
of this document. Deferred entries are research candidates, not instructions to
extend completed task IDs after finishing the selected work.

## Reuse evidence and corrections

The research's architecture-level coverage is broader than the shipped command
surface. Recheck the relevant boundary when implementing, using this baseline:

| Evidence in the current tree | Planning consequence |
|---|---|
| [Tool schemas](../../src/crates/vcp-tools/src/schema.rs) advertise literal `vcp_search` and whole-file UTF-8 `vcp_read` with `max_bytes`; [search](../../src/crates/vcp-tools/src/read.rs) already reports completeness, exclusions, path, line and source version | CR-02a/CC-04 extend one bounded tool surface and preserve its disclosure and authorization contracts |
| [ChildSpec](../../src/crates/vcp-domain/src/agents.rs) already carries a role, exact model identity, scope, root allocation and mode; `ReadOnly` permits only `EffectClass::Read` | CR-03 adds convenience templates; a read-only `shell-runner` or process verifier would violate the current contract |
| [The child workspace guide](../development/p7-child-workspaces.md) documents canonical graph admission, dirty snapshots, `.vcp-child-owner` registration, no-hook materialization, explicit recovery and `/agents` controls | Do not rebuild delegation or ownership markers. Child process checks still require qualified filesystem enforcement; changing the working directory or installing dependencies does not supply it |
| The same guide records three-way integration through parent-version-bound edits and current-parent verification; automatic worktree removal is absent | CR-08 completes deliberate cleanup already required by P7-05. Missing child dependencies do not invalidate honest parent verification |
| [The packaged review-debug skill](../../src/skills/builtin/review-debug/SKILL.md) already asks for a reproduction, root cause, evidence and location/trigger/consequence findings | CR-06 and CR-12a refine and qualify this content; they do not introduce another skill family |
| [The tool path checker](../../src/crates/vcp-tools/src/lib.rs) rejects `.git` path components; P2 policy and child authority are implemented | CR-10a checks end-to-end enforcement against the actual policy, not an assumed absence of protected-path handling |
| [P2 acceptance](../evaluations/p2-completion.md), [MCP final fault evidence](../evaluations/p7-03-final-faults.md) and [P6 completion](../evaluations/p6-completion.md) already exist | Reuse compaction, MCP authentication handling and conservative routing. Keep rejected optional advice disabled |
| [The terminal parser](../../src/crates/vcp-cli/src/terminal.rs) implements steering and `/agents`; plan autonomy exists without the proposed `/plan`, `/review`, `/rewind` or `/index` commands | A command in the research is a proposal, not evidence of implementation; selected work should fit the established interaction first |

## Preservation contract for accepted behavior

All selected refinements use the existing controller, broker, canonical stores,
root ledger and lifecycle. Repository content, model output, role templates and
skill instructions grant no authority. Preserve prepared-operation identity,
current-source checks, pause/cancel fencing and uncertain-effect reconciliation.
Verification judges the current parent result; findings and interpreted output
cannot turn an unrun or failed check into a pass.

Version changed schemas, profiles and catalog content; retain old-record and
omitted-option compatibility where those boundaries change. Keep full evidence
separate from bounded previews, enforce current access and retention at read,
and preserve required recovery/accounting references during cleanup. Existing
root caps cover all child and helper work. No additional model call, index,
service, foreign configuration format or permission path is implied by a label
in the research. Apply the existing P7/P8 suites to changed boundaries and retain
the original acceptance evidence for its original revision.

## Integration order and acceptance ownership

| Open owner | Selected work to complete | Evidence and downstream use |
|---|---|---|
| [P7-02](13-skills-and-mcp.md#p7-02--built-in-skill-catalog) | CR-02a/CC-04 bounded source navigation; CR-06 debug guidance; the Claude Code foreground-execution refinements | Changed tool contracts, packaged skill manifests and the existing U01–U03/U08 usefulness/toolchain matrix; foundational P2 fixtures are regression evidence |
| [P7-04](14-visible-delegation.md#p7-04--graph-and-workspace-ownership) | CR-03 read-only templates; CR-08 accurate readiness and setup limitations | E12/E15/U06 with real scopes, snapshot identities, root reservations and missing-input cases |
| [P7-05](14-visible-delegation.md#p7-05--integration-and-review) | CR-12a/CC-14a findings on current changes; CR-08 reference-checked cleanup | U02/U03/E15, human-edit races, retained results and current-parent verification; use M6 only within its existing gate |
| [P7-06](14-visible-delegation.md#p7-06--commentary-controls-and-recovery) | Template selection and readiness/cleanup reasons in existing child views | U06 in TUI/JSONL, concise progress, explicit pause/recovery and zero dispatch from status reads |
| [P8-01](15-integration-and-release.md#p8-01--native-windows-support-matrix) / [P8-02](15-integration-and-release.md#p8-02--recovery-and-portability-campaign) | Qualify the selected tool/delegation changes; CR-10a/CC-08 boundary cases | Native path, process, pause and fault evidence; both-store reopen where canonical state changes |
| [P8-03](15-integration-and-release.md#p8-03--full-history-and-encryption-review) | CR-13 reuse checks and CC-03 raw-output versus decoded-preview inspection | Full artifacts remain inspectable beyond prompt tails; compaction/pruning gaps and MCP failures are truthful; no hidden requests |
| [P8-04](15-integration-and-release.md#p8-04--windows-distribution) / [P8-05](15-integration-and-release.md#p8-05--owner-acceptance-and-release-evaluation) | Package selected commands/catalog revisions and record their final disposition | Exact-artifact install/upgrade evidence and developer tasks; no deferred research idea becomes a release blocker |

Work within each owner's existing prerequisites. These are internal increments,
not graph edges between the supplements. Coordinate the CR-02a/CC-04 tool schema
change with the Claude Code process changes when they share catalog fixtures, but
do not delay an independently reviewable fix merely to produce one large PR.
Existing [M5/M6/M8 follow-ups](21-markov-integration.md) retain their own gates;
none gains a dependency on a deferred Cursor feature.

## CR-02a — Bounded search and ranged reads

Owner: P7-02. Fold CC-04 into this work rather than adding another navigation
tool family. Start from `vcp-tools::{schema,read}` and the current preparation,
source-version and broker checks.

1. Add explicit literal/regex selection and a bounded path filter to
   `vcp_search`; literal remains the default. Reuse an existing suitable regex
   dependency where possible. Bound pattern/compiled size, discovered files,
   scanned bytes and returned matches; honor cancellation. Invalid or exhausted
   searches return an explicit error or incomplete result, never an empty success.
2. Add explicit line ranges to `vcp_read`, with a byte ceiling, source version,
   returned range and a clear continuation when more text is available. Omitted
   ranges retain current whole-file behavior, including existing too-large and
   unsupported-encoding errors. Do not silently auto-page or claim the model saw
   the complete file.
3. Expose bounded filename-pattern discovery through the existing discovery/list
   boundary only if content-search path filters cannot answer the P7-02 fixture.
   Keep normalized-path ordering deterministic. Avoid modification-time ordering,
   a second scanner or a new indexed search service.
4. Preserve scoped roots, ignore handling, version probes and reparse-point
   checks. Changing source between range pages must be visible; preparation must
   retain the same version/authority fences as existing reads.
5. Update skill guidance to search, then read the relevant range and nearby
   contract. Normal questions should not require developers to select a search
   mode manually; available tools and current task evidence guide the agent.

Acceptance: compare results with an independent full-scan fixture over regex
alternation, anchors, Unicode, CRLF, invalid patterns, empty results and hit/byte
limits. Test first/last/out-of-range reads and a file changed between pages.
Include ignored and outside-scope sentinel files. Existing default request/result
behavior remains compatible; the new advertised schema intentionally changes its
digest and invalidates stale preparations. Record that revision and rerun the
affected retained-loop cases rather than claiming serialized requests stayed
byte-identical across a catalog change. U01/U02 measure whether fewer irrelevant
bytes improve the result, not merely whether a new tool was called.

## CR-03 — Small helper role defaults

Owners: P7-04/P7-06, with P7-02's existing skill content. Offer `explore` and
`review` as bounded defaults in the existing `/agents` workflow: objective,
read scope, ordinary root-budget allocation and the current qualified model.
Retain the explicit specification path for advanced use. Routine selection
should not require developers to author authority or grant JSON.

Resolve templates into ordinary `ChildSpec` assignments at admission and retain
the chosen template revision with the assignment evidence. `explore` returns
relevant path/range references and a short synthesis; `review` returns CR-12a
findings. Both are read-only. They cannot run processes, patch files, select a
different model behind the user's back or expand scope. Existing exact model
pins and root accounting remain authoritative.

Show objective, scope, model, state and cost through the current attributed child
view. Avoid an extra confirmation for a child already admitted by the user's
authority and cap. Do not delegate a trivial search merely because a template
exists. A process-running verifier remains ordinary separately authorized work
under current isolation rules; do not weaken `ReadOnly` to create that role.

Acceptance: preserve the existing explicit delegation path and free-form role
labels. Exercise template admission, stale template/source revisions, denied
writes/processes, cancellation and parent pause. Compare one broad exploration
and one seeded review with the single-agent baseline, counting all helper cost
and parent-context tokens. Keep the simpler baseline when delegation adds cost
without useful evidence; templates are a convenience, not an enabled routing
classifier.

## CR-08 — Worktree readiness and safe cleanup

Owners: P7-04/P7-05/P7-06. Fold CC-15 into the existing registration and recovery
path described in [the child workspace guide](../development/p7-child-workspaces.md).

1. Distinguish materialization readiness from toolchain/check availability.
   Report missing required inputs, unavailable tools and unsupported process
   isolation on the child, with the exact check not run. Preserve verified dirty
   parent capture and the registered base; do not add a second base override.
2. Use ordinary scoped inputs and brokered operations for any authorized setup.
   Do not execute a newly discovered repository setup file, copy credentials or
   ignored environment files, or install dependencies as an implicit side effect
   of creating a child. Process execution still needs qualified host filesystem
   enforcement. Where child checks are unavailable, retain that limitation and
   run applicable checks on the integrated parent through the existing path.
3. Complete deliberate cleanup of registered disposable roots through the child
   lifecycle. Present eligible roots, retained results and reasons for keeping
   others. Before removal, revalidate the exact absolute path, native identity,
   ownership marker, registration and absence of live/recovery references.
4. Keep roots with unresolved unintegrated or unretained work, active/unknown effects,
   unresolved liabilities or an unsettled cleanup operation. Capture required
   result artifacts before releasing storage. Explicitly rejected changes may be
   cleaned through the deliberate lifecycle action once their result artifacts
   are retained and no live reference remains. Refuse substituted paths, junction
   escapes and unrelated user worktrees. A blocked cleanup is a visible diagnostic,
   not permission to remove a parent directory.
5. Expose status and deliberate cleanup within `/agents`; no timer, machine-wide
   eviction count or independent cleanup daemon. Pause/recovery fences apply to
   cleanup, and reopening or inspecting a task cannot initiate deletion.

Acceptance: dirty parent and scoped untracked inputs remain byte-identical;
missing setup produces honest not-run checks; inherited child policy cannot run
an ungranted setup operation. Interrupt cleanup before/after directory removal
and canonical acknowledgement, then reconcile without deleting another path.
Exercise replaced markers, live references, unintegrated edits, locked native
files and moved roots. Child output and cost history remain available after
successful cleanup. Optional worktree lock/sparse-checkout optimizations and
executable setup recipes remain deferred until a concrete fixture requires them.

## CR-12a — Useful local review findings

Owner: P7-05, with the `review-debug` skill and CR-03 read-only template. Fold
CC-14a's useful finding structure into existing child result artifacts and
TUI/JSONL output; do not add a finding tool or separate review store.

Make the selected base, current workspace fingerprint and examined paths visible.
Use the user's requested diff when supplied; otherwise explain the bounded scope
chosen from current changes. Do not silently review only changes after a saved
branch cursor. Check relevant callers, contracts and tests when a finding depends
on them, within the granted scope.

Each finding carries location, trigger, consequence, supporting source/evidence
references and uncertainty. Distinguish demonstrated defects from suggestions.
Diff-hunk overlap can be computed, but it does not prove the change caused the
defect: mark introduced-by-change as unknown unless comparison with the base or
other evidence supports it. An empty finding set means no supported findings in
the examined scope, not guaranteed correctness.

Reuse existing instruction precedence and activated skill revisions. New
`REVIEW.md` discovery, automatic rule learning and a default second-model review
pass are outside this increment. Preserve useful findings when a child's patch
is rejected. Parent integration must still pass verification against its current
fingerprint; a review finding or child check cannot satisfy that gate.

Do not cache acceptance by Git patch identity alone: surrounding code,
dependencies, permissions and instructions can change while the patch stays the
same. Historical findings may be displayed with their examined revisions and
current access checks; they cannot silently replace a new review. A future cache
needs a complete dependency key and invalidation evidence before adoption.

Acceptance: use U02 seeded defects and benign controls, a pre-existing defect,
unchanged diff with changed surrounding code, stale findings after a human edit,
and a passing child whose integrated parent fails. Read-only review leaves source
and index bytes unchanged. Both TUI and JSONL show the same findings, scope and
uncertainty; record false positives and total cost in the existing usefulness
report.

## CR-06 — Evidence-first debug guidance

Owner: P7-02. Refine the existing `review-debug` skill only where its current
reproduction and evidence guidance is insufficient. Use a short sequence:
identify the failing contract, state the hypothesis, reproduce or collect bounded
evidence, make the smallest fix, rerun the relevant check and inspect the final
diff for temporary instrumentation.

Prefer existing tests and logs. Add temporary instrumentation only when it will
resolve a concrete uncertainty, through ordinary prepared edits and authorized
execution. Record its exact paths and intended removal with the task's evidence;
do not create a new debug directory, network collector or mandatory logging
format. Reproduction that the agent can perform under current grants should not
be handed back to the developer. Ask for developer input only when it is actually
unavailable to the agent.

Before completion, remove temporary changes through version-checked edits and
report anything deliberately retained or blocked by a concurrent human change.
On resume, inspect the current diff and retained evidence before continuing.
This is a qualified skill procedure; it does not claim an engine-wide guarantee
that every instrumentation edit is automatically tagged or removed.

Acceptance: a seeded failure preserves the original evidence, produces a useful
fix and leaves no unintended debug output. Include missing reproduction access,
an interrupted instrumented run and concurrent human edits. Use an external
fixture check of final files and check results; do not grade by exact prose or
require a fixed-size patch. Update the packaged skill/catalog hashes when its
content changes, then run the existing P7-02 packaging and usefulness gates.

## CR-10a — Qualify existing trust boundaries

Owner: P8-02, with the P8-01 host matrix. CC-08 contributes adversarial cases,
not a new preset or approval circuit breaker. Inspect current P2 policy,
`vcp-tools` path validation and inherited child enforcement before choosing
expected outcomes.

Exercise repository text and tool output attempting to modify protected Git or
VCP control files, change ignore policy, escape registered roots, remove files
outside a granted scope, or turn a repository setup script into trusted authority.
Test the actual file and process paths, including Windows junctions and child
root projection. Use broker dispatch counts and independent sentinel bytes as
the oracle, not only policy-decision records.

Preserve authorized ordinary edits and deletions under the selected autonomy
and grants. Denied effects stay denied; uncertain effects remain visible and
are reconciled before retry. If a fixture exposes a breach of an existing
contract, fix it as part of the open qualification work and rerun the affected
baseline. Changing the intended permission model is outside this selection;
neither a research recommendation nor a restrictive-only label authorizes it.

## Qualification, documentation and exit

P8-03 includes CR-13's useful reuse checks: an oversized tool result remains
accessible through retained artifact inspection after its prompt tail is
truncated; compaction preserves source attribution and pruned ranges expose a
gap; MCP authentication/failure state is visible through the existing diagnostic
surface. Inspection must neither restore pruned content nor start provider/tool
work. These checks do not promise a new model-facing history search capability.

Run the changed boundary first. Read/search and skill-only changes need their
targeted contracts; canonical assignment, cleanup or finding-record changes also
need both-store reopen and relevant fault cases. Add older-record compatibility
checks only where a schema changes. Model-visible schemas/content receive new
revisions; retain historical evaluation validity for its original configuration
and rerun affected comparisons before making new claims.

Keep developer output concise: show the next useful action, actual blockers,
relevant findings, checks and cost. Put full transcripts and provenance in the
existing inspectors. No background model call, learned policy, new data retention
or paid evaluation budget is authorized by this plan. Live usefulness work uses
the owning campaign's explicitly configured cap and counts unsuccessful work.

Record selected increments in the [follow-up ledger](20-traceability.md), with
owner, implementation revision, exact checks, pass/fail/not-run and remaining
acceptance. P8-04 packages the exact selected catalog/tool revisions; P8-05 joins
their evidence with existing owner acceptance. A selected delta remains open
until its owner has evidence, while deferred candidates remain explicitly outside
the release. Documentation updates and green routine CI are not product
qualification.
