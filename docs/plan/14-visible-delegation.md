# 14 — Task graphs, isolated child work and visible progress

Status: implementation and qualification in progress; see [the implemented owner path and remaining gates](../development/p7-child-workspaces.md). Owns P7-04, P7-05 and P7-06. Requires grouped escalation P6-03 and P2-07 recovery; integration needs current-result verification, and visible progress needs P3-02/P3-04. Architecture section 16 governs delegation.

## Code organization

Use `vcp-engine/agents/{graph,scheduler,scope,result,integration,progress,recovery}` with `vcp-repository/{worktree,dirty_snapshot,merge}` and existing root ledger/services. The CLI adds child list/follow/focus views to the same event stream. A child is a bounded task node, not an independent process with its own uncontrolled provider access or memory store.

`ChildSpec` includes parent/root IDs, objective, role/model policy, scope, authority intersection, base/dirty snapshot, allocation/deadline/depth and acceptance criteria. `ChildResult` includes base and result fingerprints, changed paths/patch, findings, evidence/checks, unresolved issues and charges. `AgentProgress` carries attributed state/stage/commentary, model/group, tool and known/reserved cost.

Use [the child graph design](../architecture/routing-extensions-design.md#child-graph-and-workspace-snapshots)
and [delegation ADR-010](../adr/010-visible-delegation.md) to qualify concrete
snapshot, scheduling and integration choices. Children reuse the P1 task states,
store and root ledger; add stage/reason data rather than inventing a disconnected
child state machine. Root and child commands share revision, authority and durable
acknowledgement requirements.

Optional [P6 decision advice](../architecture/decision-evaluation-design.md#consumer-boundaries)
may suggest delegation or extra review. P7-04/05 still checks the concrete subtask,
scope, current evidence, allocations and integration acceptance. A probability
cannot authorize a child, bypass a required review or certify its patch. Test
disabled, abstaining, misleading and stale advice with real graph admission and
root accounting; retain these cases for P8 release qualification.

## P7-04 — Graph and workspace ownership

**Planned reuse:** [M8](21-markov-integration.md#m8m10--reuse-and-qualification-campaigns)
adds qualified child/support/integration forecasts to allocation explanations when
evidence exists. Atomic root admission and protected verification remain decisive;
missing child-specific evidence preserves baseline allocation. Do not count child
charges again in parent forecast totals or require an optional fit for delegation.

1. Implement node states and dependency edges with bounded depth/concurrency, cancellation propagation and resource-conflict scheduling. Parent budget remains authoritative; child allocations subdivide it and every request still reserves atomically.
2. Select delegation only for a concrete useful subtask; simple tasks may stay single-agent. Support read-only helpers and isolated write children for release.
3. Materialize a child's base from the intended commit plus relevant staged/unstaged/untracked parent snapshot. A worktree created from HEAD alone is not equivalent to the user's dirty workspace.
4. Track file/worktree ownership. Overlapping write sets serialize or stay isolated until integration. Non-Git folders use a qualified isolated snapshot or serialized path ownership; do not silently allow concurrent shared writes.
5. Persist graph, snapshots, grants and result refs. The child cannot expand inherited authority or access unrelated workspace memory.

Test overlapping/disjoint write sets, non-Git work, nested child cap, two near-budget-limit children, dirty parent state and parent pause before child dispatch. Observe actual worktree/file effects and root accounting, not merely scheduler status.

**Construction sequence.** Define graph commands for create/update dependency,
assignment, pause/cancel and result submission. Validate same-root membership,
acyclicity, maximum depth, current parent status, acceptance and requested scope
before committing a graph revision. Make scheduling eligibility a pure projection
of satisfied dependencies, current grants, resources, allocation and owner liveness.
Persist the selected assignment before launching work; do not create a scheduler
worker that can call the gateway directly.

Compute child authority as the intersection of current root/parent scope, assignment
scope and host enforcement. Tag each model attempt/tool run with root and child IDs.
Allocation limits subdivide the root cap and do not count as settled charges;
reconcile active reservations and unknown liabilities before releasing capacity.
Revalidate a queued child after parent steering or a grant change. Read-only review
assignments receive no write capability merely because their model asks for one.

Implement `WorkspaceSnapshot` with base commit, staged/index representation,
tracked working versions and explicitly scoped untracked artifacts. Fingerprint
before and after capture to detect concurrent user writes; bound retries and expose
an unstable snapshot instead of claiming an atomic directory copy. Excluded ignored
or sensitive paths stay excluded and missing required inputs become explicit setup
conditions. Do not commit or stash the user's edits to obtain an easier base.

Register each disposable worktree/copy by absolute path, workspace/host, owner node,
base fingerprint and recovery refs. Materialize the snapshot and verify its observed
content before child dispatch. Enforce actual write scope through the broker, not
only declared write sets. For non-Git work, use qualified copy isolation or serialize
ownership; unsupported isolation is a visible scheduling constraint.

Fixtures must independently compare staged, unstaged and untracked parent contents
before/after child creation; include a file changing during snapshot collection,
path overlap discovered after assignment, ignored local input and a missing base.
Crash after graph commit but before launch must not create duplicate nodes or lose
allocations. Follow [workspace ownership](../architecture/vcp-what.md#162-workspace-ownership)
and [root reservations](../architecture/vcp-what.md#81-root-ledger-and-reservations).

## P7-05 — Integration and review

Apply [M6 verification ordering](21-markov-integration.md#m6--verification-order)
only among ready checks against the integrated parent fingerprint. M8 forecasts
must include integration and final verification; they cannot accept a child patch
or substitute child-state passes for parent acceptance.

1. Validate the child packet's scope, base fingerprint and reported changes against actual artifacts/worktree state. Reject edits outside its declared authority and preserve useful read-only findings.
2. Check the parent has not changed since the selected integration base. Apply a conflict-aware patch/merge through prepared edit policy; conflicting or partially applied results are explicit.
3. Run appropriate checks against the integrated result. Prior child tests remain evidence about the child state, not proof of the current parent state. Failed integration cannot produce task completion.
4. Keep child results/history/cost after failures and cancellation. Remove disposable worktrees only when no active/recovery/artifact reference depends on them; never delete an unrelated user worktree from a guessed path.

Test a passing child whose merged result fails, concurrent human edits, overlap between children, stale base, malformed result packet, review-only findings and cancellation during integration. E05/E15/U02/U03 apply.

**Construction sequence.** Build a result validator that reads actual child
artifacts/worktree state, computes changed paths and checks them against assignment
scope and base identity. Treat child self-reported checks and cost as references to
canonical evidence, not authority to mark a task complete. A useful finding can be
accepted separately from a rejected patch, with its examined revision visible.

Prepare integration using a three-way comparison of child base, child result and
current parent. Preserve the user's index versus working-tree distinction. For
non-overlapping changes prepare a fresh parent-version-bound edit; for overlap
surface conflicts and produce a new reviewed resolution. Use the normal prepared
operation/authority path for actual application. New typing or filesystem edits
between preview and apply invalidate the preconditions. No forced checkout, reset
or unconditional rollback belongs in this path.

Persist integration intent, child ancestry and per-path application receipts.
Interruption may leave partial effects; reconcile observed files before retrying.
Any compensating edit must itself be prepared against current versions. Select and
run checks against the final parent fingerprint and mark affected older verification
stale. A child pass remains child-state evidence even when all its changes appear
to merge cleanly. Parent completion requires the current integration result.

Use [integration and recovery design](../architecture/routing-extensions-design.md#integration-and-progress-recovery)
and [architecture section 16.4](../architecture/vcp-what.md#164-integration).
Inject failures before/after each changed-path write and before verification publish;
assert preserved human edits, explicit partial state, child costs/history and no
false completion. Cleanup must consult registered ownership and live references,
resolve exact absolute disposable paths and leave useful result artifacts available.
Missing permission to remove a disposable worktree is a cleanup diagnostic, not
permission to delete its parent directory.

## P7-06 — Commentary, controls and recovery

For [M8](21-markov-integration.md#m8m10--reuse-and-qualification-campaigns),
display forecast versus observed cost and any inferred regime with provenance,
uncertainty and abstention. Pause/recovery applies to local statistical work too;
no probability or quiet child status can resume scheduling.

Emit durable start, meaningful progress, waiting/blocked, verification, completion/failure and cancellation events. Include child identity, parent, objective, model/group, workspace, current tool/stage and cost. A heartbeat may report unchanged status, but must not invent work or inaccessible private reasoning.

Implement `/agents`, child focus/follow, individual pause/cancel and whole-tree control in TUI/JSONL. Concise mode may collapse intermediate output but still announces children and terminal results. Bounded queues and cursor recovery prevent one noisy child from hiding other state or exhausting memory.

On parent exit/pause, stop new descendant scheduling and reconcile active effects through the common broker. Persist full child transcripts, partial result packets, liabilities and worktree bindings. Resume validates each node/worktree before restarting; a missing child process cannot erase its work or cost.

`/pause` must also work while VCP stays open. Pause the root tree, stop admission of
new descendant model/tool work and task-scoped extraction/observer work, and
reconcile in-flight effects while
the CLI continues to show progress, costs, pending outcomes and inspectable history.
Keep `/agents`, status and read-only inspection usable in this state. Only an
explicit `/resume` or equivalent resume command revalidates and restarts work;
receiving a status query, reconnecting a view or a heartbeat cannot resume it.
The configured local pause checkpoint and receipt reconciliation remain permitted
within their existing authority; neither can start hidden model calls or task effects.

Test interleaved progress from several children, silent/blocked child, output flood, dropped UI consumer, child crash during write, parent terminal close and moved/missing worktree at resume. U06 must demonstrate no unseen scheduler continues after owner loss.

**Construction sequence.** Add durable attributed lifecycle events and projections
with child/root/parent IDs, objective, state/stage, workspace ref, model/group,
active tool, useful commentary and ledger-derived known/reserved/uncertain cost.
Record source attempt/tool IDs for evidence links. Do not derive progress from
private reasoning or trust a child's reported cost sum. Render the same semantic
records in TUI and JSONL, with child labels surviving collapsed/concise output.

Use bounded subscriber queues and cursor recovery. Coalescing high-frequency token
deltas must not discard question, blocked, failure or terminal transitions; retain
their durable stream and full observed transcript according to history policy.
Expose last observed activity and wait reason for a quiet child. A heartbeat is a
status observation and must not consume a model call merely to generate commentary.

Handle pause as a durable controller barrier: first prevent fresh dispatch across
the tree, then cancel/settle or mark unknown active work, then checkpoint node,
worktree and liability refs. Individual pause preserves unrelated siblings under
the parent's current policy; root pause blocks all descendants. Root resume preserves
independently paused or cancelled children. Resume reconciles
unknown effects and validates workspace identity, instructions, grants, catalog
and budget before rescheduling. Missing/moved worktrees remain explicit blocked
nodes with history intact until deliberately rebound or resolved.

Add a live-CLI pause fixture with active root/child attempts: retain the terminal,
issue `/pause`, inspect the tree and advance a fake scheduler clock. Assert no new
dispatch until `/resume`, then verify only reconciled eligible nodes restart.
Also test a late child result after steering, recovery from a dropped UI cursor,
individual cancellation with siblings active and a hard-close external marker.
These cases implement [architecture section 4.5](../architecture/vcp-what.md#45-terminal-close-pause-and-workspace-resume)
and [visible child work](../architecture/vcp-what.md#166-visible-sub-agent-work),
not merely event formatting.

## Exit

Run `delegation`, `recovery`, `cli`, E12/E15 and U02/U03/U06. Evidence includes event attribution, independent change observations, integrated check results and root ledger totals. Done when useful child work is visible, isolated, budgeted and recoverable, and successful integration is judged on the current parent result.
