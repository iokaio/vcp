# 06 — Windows execution, policy, prepared edits and recovery

Status: P2-03 complete with [authority acceptance](../evaluations/p2-policy-completion.md); P2-04 is in progress with [prepared native files](../development/p2-tools.md), the [process broker](../development/p2-process.md) and [owned terminals](../development/p2-pty.md). P2-07 remains planned. P2-03 depends on domain/store and the P0 Windows/reuse decisions; P2-07 also requires the session loop and projections. Architecture sections 9, 10 and 14 govern effects.

## Implementation references

The [authority coordination adapter](../development/p2-authority-coordination.md)
connects active policy/trust/grant/binding/steering changes to owned stopping and
canonical pause. It remains a trusted host interface pending the governed loop
and CLI adapters.

Read [dispatch ordering](../architecture/vcp-what.md#92-tool-dispatch-sequence),
[file/process semantics](../architecture/vcp-what.md#93-file-changes-and-editor-conflicts),
[policy decisions](../architecture/vcp-what.md#102-decision-order) and
[pause/resume](../architecture/vcp-what.md#45-terminal-close-pause-and-workspace-resume).
The proposed [engine execution design](../architecture/engine-execution-design.md#prepared-effects-and-authorization)
defines immutable operations, receipts and recovery ordering.
[ADR-004](../adr/004-edits-and-execution.md),
[ADR-005](../adr/005-autonomy-and-isolation.md) and
[ADR-016](../adr/016-history-and-pause.md) track the relevant qualification work.
The qualified authority default is workspace mode. Broader Windows isolation
mechanisms remain measured selections under P8-01.

## Code organization

| Module | Proposed submodules | Contract |
|---|---|---|
| `vcp-policy` | `rules`, `autonomy`, `grant`, `decision`, `confirmation` | Evaluate trusted ceilings and scoped grants for one immutable operation |
| `vcp-tools` | `definition`, `prepare`, `patch`, `file`, `search`, `git`, `process`, `receipt` | Validate inputs, declare resources/effects and produce prepared invocations |
| `vcp-exec` | `broker`, `capability`, `worker`, `windows`, `pty`, `output`, `cancel` | Execute only broker-issued capabilities in controlled processes |
| `vcp-engine` | `effect_journal`, `shutdown`, `reconcile` | Durable dispatch, task-tree stop and recovery decisions |

Adapt C02/C03/C04 and G01/G03; retain upstream parser/matcher tests. A `PreparedInvocation` binds canonical arguments, tool/schema revision, operation hash, affected resources, expected file versions and steering/policy revisions. An execution capability adds allowed effects, environment, deadline and output/resource bounds. Neither object is manufactured by model text.

## P2-03 — Effective autonomy and grants

1. Implement proposed plan/ask/workspace/autonomous modes as named policies, with actual rules and default choice recorded in ADR-005. Keep mode, sandbox enforcement, headless interactivity and money limits independent.
2. Evaluate trusted user/host denials, applicable grants, operation identity and scoped approval. More permissive project text cannot erase a denial. Matching shell text alone is not a sufficient execution grant.
3. Bind pending questions to argument/path/schema hash, policy revision, actor, scope and expiry. Reject duplicate conflicting, late or stale answers. Honor existing valid grants rather than repeatedly prompting.
4. For headless requests that need a decision, persist waiting state and return the defined result instead of hanging or auto-approving.

Make evaluation a pure function over prepared invocation, effective policy,
existing grants and declared platform capabilities. Return structured allow,
deny or question with rule origins and failed requirements. Keep money admission
outside this function: a broad tool grant neither pays for a model request nor
raises its cap. Keep isolation separate too: user approval does not make an
unsupported network restriction technically enforced.

Canonicalize operations before hashing/approval. Include execution host, root
binding, arguments, explicit shell versus direct executable, schema version,
resource set and relevant source revisions. Resolve executable/path identity
under policy; a shell token match cannot authorize arbitrary redirections or
later script substitutions. If parsing cannot establish the requested effect
scope, classify it conservatively instead of treating uncertainty as read-only.

A proposed `QuestionRecord` has question/operation IDs, immutable digest,
authorized actor/controller, scope, policy revision, expiry and pending/resolved
state. Resolve it with a compare-and-set in canonical storage. Repeated identical
answers return the recorded result; conflicting or stale answers grant nothing.
At dispatch, recheck operation identity and grant validity even when the answer
was accepted earlier. Do not repeatedly prompt for an existing valid grant.

Build fixtures that alter only one approval-bound field at a time: executable,
argument, junction target, schema, host, policy or expiry. Observe actual broker
dispatch counts, not just the policy service's return value. Pausing with a
question pending keeps it visible but disables any effect dispatch; resume
revalidates it rather than accepting an expired highlighted choice.

Tests: deny versus allow precedence, command rewrites after approval, changed MCP schema, redirection/argument ambiguity, new path outside grant, expiration and repeated confirmation. Assert actual dispatch counts at the broker. E06/R03 apply.

## P2-04 — Tools and Windows worker

1. Implement read/list/search tools with canonical root checks and bounded output. Escape untrusted terminal control sequences at presentation; keep underlying captured bytes attributed.
2. Adapt patch parsing into an edit plan. Read expected versions, preserve CRLF/encoding and staged/unstaged/untracked state, then revalidate immediately before each write. Detect rename/delete/case and link changes. Never use a rollback that overwrites subsequent human edits.
3. Record multi-file outcomes individually. Where atomic multi-file application is unavailable, report partial effects and keep recovery receipts; avoid promising filesystem compare-and-swap stronger than the tested primitive.
4. Reuse native Windows process/PTY/job controls with explicit executable, argument vector, working directory, inherited handles and filtered environment. Secret handles are scoped; the child does not inherit all engine credentials.
5. Bound process count, execution time and output spooling. Cancel process trees and report residual effects independently of exit status. Git/publishing/network tools use the same policy path as file effects.

Implement preparation as a side-effect-free edit plan plus permitted reads. It
contains every affected path, expected before bytes/hash, intended after bytes,
encoding/line-ending handling and rename/delete ordering. Fail ambiguous matches
before writing anything. Validate the entire plan first, then recheck each
path/identity/version near its actual write; a long plan cannot rely only on its
initial validation.

| Effect stage | Durable or observable record |
|---|---|
| Prepared | Immutable invocation and complete proposed diff |
| Authorized | Policy/grant origins and bound operation identity |
| Dispatch recorded | Execution identity, owner generation and required capabilities |
| Per-file mutation | Before/intended-after/observed-after identities and certainty |
| Process activity | Worker nonce, actual process identity, output offsets and cancellation state |
| Outcome | Exit status, per-resource effects, full artifacts and unresolved remainder |

Use qualified per-file staging/replacement primitives and document their actual
Windows concurrency envelope. Internal locks coordinate VCP workers but cannot
exclude arbitrary editors. If a failure follows a successful first file, report
partial application and preserve receipts. Automatic rollback must never restore
old content over a later user edit. Git index mutations are separate authorized
operations; applying a patch should not clean, reset or stage unrelated work.

Worker bootstrap verifies broker identity and the scoped capability; it receives
only explicit arguments, working directory, allowed environment/handles and
limits. Keep provider keys in the trusted engine. Test direct executable calls
and explicitly selected shell/PTY modes separately because quoting and handle
semantics differ. Validate that native process/job controls actually cover
children created near startup and cancellation; do not infer success from a
parent process disappearing.

Spool stdout/stderr in independent ordered streams with bounded in-memory tails.
Output-limit or disk failure triggers visible cancellation/partial-output state,
not silent truncation of claimed full capture. Report surviving descendants and
unknown filesystem/network effects separately from process exit status. Requests
for unsupported isolation fail or use an explicitly configured reduced-isolation
workflow; a worktree is not an OS sandbox.

Tests use real temporary Windows roots and helper executables: spaces/Unicode, quoting/metacharacters, CRLF, locked file, long path, directory replacement, junction escape/race, stale file hash, partial rename failure, noisy child and surviving grandchild. Verify actual outside-root markers remain untouched, edited bytes match intent, and test reports state which OS controls were enforced. E05/E07/E08/R02/R04/R05 apply.

## P2-07 — Pause and unknown-effect reconciliation

Use the following durable order: prepared/authorized operation, dispatch intent commit, worker dispatch, observed outcome, outcome commit. Place fault-injection barriers before/after every step. Only supported idempotent/reconciled operations may be retried; a missing success record is not permission to repeat an effect.

1. On close, explicit pause or controlling-client loss, mark stopping, block new model/tool/maintenance/child scheduling and request bounded cancellation. Persist partial artifacts and charge uncertainty.
2. Periodically checkpoint during work; forced process termination cannot depend on a graceful exit hook. Test actual console closure separately from synthetic connection loss.
3. On restart, inspect durable intent and owned process identity, actual file versions/receipts and available provider status. Guard against PID reuse before process operations. Classify outcomes as known, reconciled or unknown.
4. Reconstruct paused root/child state and explain required decisions. Resume revalidates current root bindings, policy, instructions, files, model capabilities and available budget.

### Pause while the CLI remains open

`/pause` is a first-class control for temporarily stopping VCP without closing
the terminal. It uses the same internal pause command as
`vcp tasks pause <task-id>` and the structured CLI adapter. The root task and its
descendants stop admitting new model calls, tools, retry timers, maintenance and
child work. In-flight streams/processes receive bounded cancellation and
reconciliation; pause does not promise to freeze arbitrary external programs at
an instruction boundary.

Keep the owning CLI open in an explicit paused view. It must continue accepting
status, cost, history and evidence inspection, show unresolved effects/charges and
pending questions, and allow deliberate `/resume` in the same process. Answering
a pending question while paused cannot restart work. Repeated pause commands are
idempotent and preserve the existing recovery record. `/cancel` ends the requested
work according to cancellation semantics; pause retains its objective and
continuation path. Closing the CLI is another trigger for pause, not a
prerequisite for it.

P0/P3 map the structured pause command to the owning CLI's structured input or a
qualified private owner-control channel. A separate CLI invocation cannot open a
second writer or bypass controller identity. If it cannot contact the owner,
return an explicit unavailable/conflict result. This private control path does
not require the deferred public attach server or SDK.

### Admission fencing and restart decisions

Implement shutdown as a controller admission generation: mark pause requested,
fence queued callbacks, signal descendants and in-flight work, collect bounded
results, then commit the durable paused summary. An internal `stopping` phase is
not an additional public task-state enum. A late result can append evidence or
settle money but cannot enqueue another model/tool step. Background maintenance
must participate in the same owner-lifetime fence.

Maintain a recovery scan keyed by durable dispatch/attempt IDs. For each pending
record, select a reconciler by effect class: file hashes/identity, live worker
nonce/process identity, explicit remote receipt, or unknown. Reconcile from
observations instead of assuming that missing success means no effect. Unknown
outcomes block dependent mutations; unrelated authorized reads can remain usable
while the user inspects/reconciles the task.

Resume is a fresh controller command with an expected paused revision. It checks
roots, live workspace changes, applicable instructions/grants, pending effects,
model/catalog state and retained root budget. A denied or unavailable prerequisite
leaves a visible paused/blocked continuation path. It never jumps into a saved
worker stack or blindly resubmits a non-idempotent dispatch intent.

Record inherited parent pause separately from an explicit child pause or
cancellation. Resuming a parent removes its inherited hold only; independently
paused/cancelled children stay stopped. Test a parent with one inherited-paused
child and one explicitly paused child, then verify only eligible work can resume.

Tests: kill before dispatch, after external effect but before receipt, during output write, during reservation settlement and during pause checkpoint. A non-idempotent marker fixture records the number of actual effects outside VCP state. On reopen it must never gain a second marker solely from automatic replay. Unknown usage remains reserved. U06/E08/E10/E12 apply.

Add open-CLI tests for pause during model streaming, a running process, queued
child work and pending input; repeated pause; status/history/cost inspection
while paused; and resume in that same process. Assert no new request/process
starts after the admission fence, late usage is still accounted for, and no
queued callback bypasses resume revalidation. Test actual console closure and
forced termination separately: only the former may give graceful shutdown time.

## Exit

Run `tools`, `windows`, `recovery` and relevant `store` conformance. Evidence includes policy/dispatch receipts, process-tree observations, current file fingerprints and post-reopen state. Any silent overwrite, unauthorized effect, hidden surviving scheduler or replayed uncertain effect blocks completion.
