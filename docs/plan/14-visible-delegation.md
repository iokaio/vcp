# 14 — Task graphs, isolated child work and visible progress

Status: planned. Owns P7-04, P7-05 and P7-06. Requires grouped escalation P6-03 and P2-07 recovery; integration needs current-result verification, and visible progress needs P3-02/P3-04. Architecture section 16 governs delegation.

## Code organization

Use `vcp-engine/agents/{graph,scheduler,scope,result,integration,progress,recovery}` with `vcp-repository/{worktree,dirty_snapshot,merge}` and existing root ledger/services. The CLI adds child list/follow/focus views to the same event stream. A child is a bounded task node, not an independent process with its own uncontrolled provider access or memory store.

`ChildSpec` includes parent/root IDs, objective, role/model policy, scope, authority intersection, base/dirty snapshot, allocation/deadline/depth and acceptance criteria. `ChildResult` includes base and result fingerprints, changed paths/patch, findings, evidence/checks, unresolved issues and charges. `AgentProgress` carries attributed state/stage/commentary, model/group, tool and known/reserved cost.

## P7-04 — Graph and workspace ownership

1. Implement node states and dependency edges with bounded depth/concurrency, cancellation propagation and resource-conflict scheduling. Parent budget remains authoritative; child allocations subdivide it and every request still reserves atomically.
2. Select delegation only for a concrete useful subtask; simple tasks may stay single-agent. Support read-only helpers and isolated write children for release.
3. Materialize a child's base from the intended commit plus relevant staged/unstaged/untracked parent snapshot. A worktree created from HEAD alone is not equivalent to the user's dirty workspace.
4. Track file/worktree ownership. Overlapping write sets serialize or stay isolated until integration. Non-Git folders use a qualified isolated snapshot or serialized path ownership; do not silently allow concurrent shared writes.
5. Persist graph, snapshots, grants and result refs. The child cannot expand inherited authority or access unrelated workspace memory.

Test overlapping/disjoint write sets, non-Git work, nested child cap, two near-budget-limit children, dirty parent state and parent pause before child dispatch. Observe actual worktree/file effects and root accounting, not merely scheduler status.

## P7-05 — Integration and review

1. Validate the child packet's scope, base fingerprint and reported changes against actual artifacts/worktree state. Reject edits outside its declared authority and preserve useful read-only findings.
2. Check the parent has not changed since the selected integration base. Apply a conflict-aware patch/merge through prepared edit policy; conflicting or partially applied results are explicit.
3. Run appropriate checks against the integrated result. Prior child tests remain evidence about the child state, not proof of the current parent state. Failed integration cannot produce task completion.
4. Keep child results/history/cost after failures and cancellation. Remove disposable worktrees only when no active/recovery/artifact reference depends on them; never delete an unrelated user worktree from a guessed path.

Test a passing child whose merged result fails, concurrent human edits, overlap between children, stale base, malformed result packet, review-only findings and cancellation during integration. E05/E15/U02/U03 apply.

## P7-06 — Commentary, controls and recovery

Emit durable start, meaningful progress, waiting/blocked, verification, completion/failure and cancellation events. Include child identity, parent, objective, model/group, workspace, current tool/stage and cost. A heartbeat may report unchanged status, but must not invent work or inaccessible private reasoning.

Implement `/agents`, child focus/follow, individual pause/cancel and whole-tree control in TUI/JSONL. Concise mode may collapse intermediate output but still announces children and terminal results. Bounded queues and cursor recovery prevent one noisy child from hiding other state or exhausting memory.

On parent exit/pause, stop new descendant scheduling and reconcile active effects through the common broker. Persist full child transcripts, partial result packets, liabilities and worktree bindings. Resume validates each node/worktree before restarting; a missing child process cannot erase its work or cost.

Test interleaved progress from several children, silent/blocked child, output flood, dropped UI consumer, child crash during write, parent terminal close and moved/missing worktree at resume. U06 must demonstrate no unseen scheduler continues after owner loss.

## Exit

Run `delegation`, `recovery`, `cli`, E12/E15 and U02/U03/U06. Evidence includes event attribution, independent change observations, integrated check results and root ledger totals. Done when useful child work is visible, isolated, budgeted and recoverable, and successful integration is judged on the current parent result.
