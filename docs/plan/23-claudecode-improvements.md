# 23 — Claude Code-derived improvements in remaining work

Status: selected planning changes; no implementation or qualification is claimed.
This supplement selects useful parts of [the research note](../research/claudecode.md)
for remaining implementation and acceptance work. It shares the
[Cursor preservation contract](22-cursor-improvements.md#preservation-contract-for-accepted-behavior),
uses existing task IDs and adds no architecture dependency. The
[task ledger](20-traceability.md#work-item-ownership-and-readiness) remains the
entry point. Completed P0–P3, P5, P6, P7-01 and P7-03 acceptance stays complete;
new work is owned by the remaining tasks named below, even when it changes
already implemented P2 support code.

The research is a source of patterns, not a second product specification.
[Architecture draft 0.4](../architecture/vcp-what.md) and recorded ADRs govern.
No Claude Code code, prompt, configuration format or default list is imported.

## Selection and developer workflow

Prioritize a developer being able to read relevant source, run the project's
checks, understand their output and review an isolated change. Reuse existing
skills, process profiles, review records, inspectors and pause controls. Avoid a
new command, confirmation, model pass or service when those surfaces can do the
job. Keep short commands and already working profiles unchanged.

The `CC-nn` identifiers retain research traceability; they are checklist labels,
not new tasks. Folded rows have one implementation contract in plan 22.

| Increment | Disposition and remaining owner | Selected value or reason to stop |
|---|---|---|
| CC-02a Declared foreground time limits | Selected — P7-02; packaged qualification P8-01 | Let a configured build or test finish beyond the current two-minute ceiling, within trusted limits and the task deadline |
| CC-03 Windows output correctness | Selected, narrowed — P7-02; packaged qualification P8-01 | Make declared non-UTF-8 output readable and disclose decoding loss without changing raw process evidence |
| CC-04 Ranged reads and path discovery | Folded into CR-02a — P7-02 | Explicit bounded source ranges and deterministic path discovery; preserve the existing whole-file contract |
| CC-08 Execution-adjacent paths and removal safety | Folded into CR-10a — P8-02 | Exercise existing authority and resolved-path boundaries; no new blanket approvals or shell parser |
| CC-14a Review evidence records | Folded into CR-12a — P7-05 | Findings with location, failure scenario and evidence; distinguish an observed result from a suggestion |
| CC-15 Worktree lifecycle | Folded into CR-08 — P7-04/05/06 | Reuse existing registration/marker checks, surface readiness problems and qualify deliberate owned cleanup |
| CC-11 Fast diagnostics | Folded into M6 and P7-02/05 verification qualification | Use an already configured compiler/linter check where useful; no language-server runtime |
| CC-01a Request-prefix diagnostics and CC-10 usage attribution | Deferred observation candidate | Existing accounting and inspectors are sufficient for release; byte-prefix similarity is not proof of provider caching, and shared context cost is not exactly attributable to individual extensions |
| CC-01b/c Cache layout, routing, fan-out timing and confirmations | Deferred | No qualified endpoint cache-lifetime or measured ordering problem; keep ADR-041's conservative routing and reservation rules |
| CC-02b Background executions and CC-12 event watch | Deferred | A new live-effect lifecycle is unnecessary to fix long foreground checks; no automatic timeout-to-background transfer |
| CC-05 Work list, CC-06 directed compaction, CC-07 rehydration | Deferred | Avoid a second plan state and extra compaction controls before M5 measures a continuity failure; preserve current deterministic compaction |
| CC-09 Session-denial command grammar | Deferred | A future control needs a demonstrated workflow gap; existing policy, grants and pause controls remain the developer surface |
| CC-11 Language-server navigation | Deferred | Server installation, lifecycle and stale-state handling need their own measured benefit; compiler checks do not require this infrastructure |
| CC-13 Orchestration recipes, CC-14b second review pass, CC-17 inherited-context children | Deferred | Finish ordinary delegation and review usefulness first; no automatic extra agents, spend or retained parent transcript |
| CC-16 Arbitrary final-result schema | Deferred to consideration in P9-01 | Existing JSONL covers current automation; a future public protocol may justify a bounded schema using ADR-028, without reopening P3-01 |
| CC-18a Convention digest | Deferred with M5 evidence | Recheck current governed-memory recall before adding an always-present context part |
| CC-18b Derivable-claim heuristic | Not selected | No deterministic derivability oracle; do not alter accepted memory from an unqualified judgment |
| CC-18c User-scope memory | Not selected | Changes workspace scope under A07/ADR-008 and needs a separate product decision |
| Transcript-judged goal loops, advisor/permission classifiers, general shell permission grammar | Not selected | Existing completion, advisory and deterministic authority contracts already own these decisions |
| Media readers, output styles and theme/status customization | Not selected | No demonstrated need in remaining Windows CLI acceptance work |
| Hosted sessions, remote control, cross-machine agent teams, browser/computer use, plugins and marketplaces | Out of scope | Keep the local single-user CLI and existing extension boundaries |

## Current implementation evidence

This is a local-source review of the September 21, 2026 baseline. Recheck the
affected boundary when implementing; the research snapshot and historical
increment reports are not current readiness ledgers.

| Boundary | Evidence and consequence |
|---|---|
| Process limits | [`vcp-tools::process`](../../src/crates/vcp-tools/src/process.rs) rejects timeouts above 120,000 ms and output limits above 8 MiB. [`vcp-lifecycle::process`](../../src/crates/vcp-lifecycle/src/process.rs) independently enforces the same maxima. Both layers must participate in CC-02a |
| Prior execution-ceiling work | [The P2 execution-ceilings increment](../evaluations/p2-execution-ceilings.md) qualified a simultaneous process-count ceiling. It does not provide configurable duration; its historical `in_progress` status is superseded by [P2 completion](../evaluations/p2-completion.md) |
| Verification | [`vcp-tools::verification`](../../src/crates/vcp-tools/src/verification.rs) requests 120,000 ms and 1 MiB. [`worker::verification`](../../src/crates/vcp-lifecycle/src/foundation/worker/verification.rs) requires a direct, nonterminal profile. Shell/PTY execution is not evidence of a successful `vcp_verify` check |
| Process presentation | [`foundation::coding`](../../src/crates/vcp-lifecycle/src/foundation/coding.rs) decodes stdout/stderr tails with lossy UTF-8 while retaining artifacts. Profile suppression (`-NoLogo -NoProfile -NonInteractive`) and null stdin for pipe processes already exist; CC-03 preserves them |
| Source reading | [`vcp-tools::schema`](../../src/crates/vcp-tools/src/schema.rs) exposes whole-file UTF-8 reads with `max_bytes`; [`read`](../../src/crates/vcp-tools/src/read.rs) has bounded directory listing and literal search. Ranges and path-pattern discovery are new CR-02a support work |
| Worktrees and review | [The current P7 implementation record](../development/p7-child-workspaces.md) supersedes this row's original snapshot: registered isolated roots, dirty/untracked snapshots, inherited authority, three-way integration, deliberate reference-checked cleanup and visible controls are implemented. [P7 completion](../evaluations/p7-interruption-exploration-2026-09-22.md) includes its passing final source-bound gate. Child process checks still need qualified filesystem enforcement; packaged P8 recovery remains separate |
| Caching and cost | Captured requests, cache charge categories and read-only inspectors exist. The [P6 disposition](../evaluations/p6-completion.md) does not qualify new statistical/advisory defaults; observation must not silently enable cache-aware routing |

## Integration order and completion

1. Under [P7-02](13-skills-and-mcp.md#p7-02--built-in-skill-catalog), finish
   useful source discovery (CR-02a/CC-04), declared long-check limits (CC-02a)
   and native output correctness (CC-03) alongside remaining toolchain and
   generation/usefulness cases. These are support changes to existing tools,
   not a reopening of P2 acceptance. Preserve the frozen fixture expectations
   in [native toolchain evidence](../evaluations/p7-02-native-toolchains.md);
   seeded failures and unavailable checks cannot become passes by reinterpretation.
2. [P7-04/05/06](14-visible-delegation.md) are complete with the worktree, review
   and connected interruption refinements and passing final native gate.
   Keep the simpler broad-exploration baseline
   where helpers add cost without useful evidence. Recipes and another review
   pass are not required for this acceptance.
3. [P8-01](15-integration-and-release.md#p8-01--native-windows-support-matrix)
   qualifies shipped process behavior on packaged native Windows;
   [P8-02](15-integration-and-release.md#p8-02--recovery-and-portability-campaign)
   exercises selected policy and recovery cases; P8-03 checks full artifacts
   against presentation; P8-05 records final disposition and the owner UX result.

Selected rows are requirements of their remaining owners. Deferred and
unselected rows are not release gates and do not authorize implementation,
new dependencies or paid trials. A selected row closes with current acceptance
evidence; omitting it requires an explicit documented plan revision. Record
support changes and old-record compatibility under the remaining owner, leaving
the historical completion ledger intact. The graph remains 68 task IDs with
56 first-release tasks in P8-05's closure.

## CC-02a — Declared foreground execution ceilings

Owner: P7-02. Acceptance consumer: P8-01. Goal: a developer can run a legitimately
long check through ordinary verification and still pause or cancel it.

Before implementation, reproduce the current limit with a direct verification
fixture and record which remaining toolchain cases need more time. Inspect all
limit checks, including tool preparation, native supervisor and task deadlines.
Do not remove a supervisor limit after changing only the model-facing request.

1. Add an optional bounded maximum duration to trusted process-profile
   configuration and an optional requested duration to explicitly configured
   verification checks. Check requests must fit the profile ceiling and the
   remaining task deadline; absence preserves current 120-second behavior.
   Choose and document a finite host maximum from the accepted execution
   contract. A repository manifest, skill or model output cannot raise it.
2. Bind duration and profile revisions into preparation, approval identity and
   verification provenance. Reject invalid or excessive requests before
   dispatch, with a diagnostic identifying the limiting profile/check/deadline.
   A developer configures a long check once through existing trusted setup;
   the feature adds no recurring confirmation or new command grammar.
3. Keep execution in the foreground and preserve process-count, output, disk,
   isolation and authority ceilings. Keep ordinary output capture and bounded
   presentation. Larger output limits are not part of this increment unless a
   reproduced toolchain case requires a separately documented bounded change.
4. Preserve admission fencing and owned-tree termination on pause, cancellation,
   deadline and owner loss. Reports distinguish timeout, output-limit stop,
   failed check, cancellation and could-not-run conditions from actual receipts;
   none satisfies verification completion. Reuse existing outcome and stop
   evidence before introducing a new record shape.

Acceptance uses a deterministic fixture whose check exceeds 120 seconds and
completes within its explicitly allowed duration. Without that allowance the
request is rejected or the existing check times out, as appropriate. Exercise
both `vcp_exec` and the real direct-profile parent `vcp_verify` path; configure
enough task time to make the profile ceiling the tested boundary. A pause during
that check meets the existing cancellation bound and leaves no live owned tree.
Also cover excessive/model-supplied limits, an earlier task deadline, output
exhaustion, changed profile identity and old configuration defaults. Reopen the
result on both stores and confirm timeout never becomes a pass. Short fixtures
cover boundary arithmetic; reserve the long native run for this behavior and
final packaged qualification. This does not qualify child process isolation.

## CC-03 — Windows process output and outcomes

Owner: P7-02. Acceptance consumers: P8-01 and P8-03. Narrow the research's broad
interpreter to reliable presentation of observed command output.

1. Where remaining toolchain fixtures show a decoding problem, allow a trusted
   profile to declare its supported output encoding. Record a concrete encoding
   or Windows code page, the decoding decision and any replacement characters.
   Do not guess an encoding or rely on an unrecorded ambient OEM code page.
   Existing profiles keep current behavior; no new global encoding default.
2. Keep raw stdout/stderr artifacts, exit codes and stop receipts intact. Decode
   the bounded tail correctly when its beginning splits a multibyte character;
   disclose omitted bytes and undecodable input. Preserve existing terminal
   presentation sanitization and treat captured escape/control sequences as
   untrusted presentation data.
3. Reuse existing `-NoProfile` and null-stdin behavior. If an explicitly
   configured PowerShell invocation needs an encoding preamble, treat changed
   arguments as a profile/operation revision, not a silent repair.
4. Keep raw exit status authoritative. A generic table keyed only by executable
   basename cannot determine the semantics of arbitrary arguments, scripts or
   wrappers. Do not add that table or let an output interpreter change a
   verification pass rule. A qualified tool-specific adapter may later explain
   a negative search result with its exact command evidence.

Acceptance covers non-ASCII paths/output, UTF-8 and any additional encoding
actually supported, malformed and split multibyte tails, colored output and a
child that reads stdin. Exercise pipe profiles on Windows PowerShell 5.1 and 7
where claimed; keep PTY cases separate. Compare raw artifact bytes outside the
decoder, verify capture/presentation differences are labelled, and prove a
failed verification remains failed. Include an unchanged-profile case and a
fresh-process inspector/reopen case. A new encoding is advertised only for
host/toolchain combinations actually qualified.

## Folded contracts

Use these single contracts rather than implementing the research proposals
independently:

| Research delta | Implementation contract | Correction to the source proposal |
|---|---|---|
| CC-04 | [CR-02a — Bounded search and ranged reads](22-cursor-improvements.md#cr-02a--bounded-search-and-ranged-reads) | An explicit range may page a file; a legacy whole-file call must not silently become a partial read. Preserve exact file-version checks and existing edit preconditions |
| CC-08 | [CR-10a — Qualify existing trust boundaries](22-cursor-improvements.md#cr-10a--qualify-existing-trust-boundaries) | Test resolved paths, trusted profile inputs and opaque-process denials. A filename classification is not a shell sandbox; routine CI/build configuration edits do not acquire blanket new approval prompts |
| CC-14a | [CR-12a — Useful local review findings](22-cursor-improvements.md#cr-12a--useful-local-review-findings) | Diff overlap can be computed; it does not prove a defect was introduced by the change. Keep causality unknown without comparative evidence. Reuse review/result records before adding a new tool |
| CC-15 | [CR-08 — Worktree readiness and safe cleanup](22-cursor-improvements.md#cr-08--worktree-readiness-and-safe-cleanup) | Ownership markers and parent dirty snapshots already exist. No new ignore-file dialect, automatic setup script, alternate base, sparse checkout or worktree-lock mechanism is selected |
| CC-11 diagnostics | [M6 — Verification order](21-markov-integration.md#m6--verification-order), with P7-02/P7-05 consumers | Evaluate configured cheap checks within the full required set. No background language server or new navigation catalog is required |

## Deferred observation and context ideas

CC-01a/CC-10 may be reconsidered only for a concrete question that existing
`/cost` and inspectors cannot answer. Prefer a read-only report over retained
request/usage evidence, with no provider request and no new canonical writes.
Use a bounded scan and label redacted/pruned/missing inputs. A common serialized
byte prefix is a request-layout observation, not a cache-hit prediction: request
IDs, JSON ordering, provider normalization and endpoint cache rules can differ.
Use provider-reported cached usage only where supplied, with unknown elsewhere.
Correlated changes in skills, context or endpoints do not prove a causal cache
loss. Attribute actual task/attempt charges once; overlapping extension/context
associations must not be presented as additive cost shares or savings.

CC-06/07/18a remain inputs to M5's existing continuity/recall comparison. A
reproduced failure may justify a bounded context change with current-source
validation and a new assembly revision. It does not justify automatic file or
skill reinjection, a new retained work list, arbitrary range compaction controls
or a cold-resume confirmation by default. Existing protected fields, history
access and governed memory remain the baseline.

If P9-01 later selects CC-16, define a bounded exact schema profile and
compatibility behavior there; validate schema configuration before paid work
and preserve the existing CLI exit contract. This document authorizes no
result-schema flag or retry spend.

## Evidence and UX acceptance

Record each selected or folded outcome once in the combined follow-up ledger,
with its remaining owner, source/catalog/profile revision and actual test
evidence. P7-02/05 usefulness cases and P8-05 owner review must cover finding
source, running a long check, reading an error, reviewing a change and pausing
ongoing work. Compare redundant calls, misleading success reports and required
manual intervention with the existing workflow; no paid comparison runs without
an explicit configured evaluation cap.

Use focused tool/process tests first, then relevant native platform,
recovery and packaged gates. Both stores apply where canonical outcomes change;
old configuration and old retained records remain readable. UI and model tails
must never be represented as full capture. This planning revision performs no
product implementation and supplies no replacement for future acceptance runs.
