# 07 — Interactive CLI, structured output and inspectors

Status: planned. Owns P3-01 through P3-04. Begin after P2 verification/recovery/continuity. Memory/history/pruning and vault UI extensions are owned by segments 09–11, not duplicate work here.

## Implementation references

Follow [CLI grammar and lifecycle](../architecture/vcp-what.md#17-cli-and-programmatic-clients),
[open-CLI pause/resume](../architecture/vcp-what.md#45-terminal-close-pause-and-workspace-resume)
and [capture truthfulness](../architecture/vcp-what.md#143-capture-policy-and-truthfulness).
The proposed [engine/inspection design](../architecture/engine-execution-design.md#verification-cli-and-inspection)
and [context provenance design](../architecture/context-provider-design.md#reproducible-context-contract)
supply internal records. [ADR-012](../adr/012-clients-and-distribution.md) and
[ADR-016](../adr/016-history-and-pause.md) track presentation/lifetime choices;
the public API and editor remain deferred.

## Code organization

Adapt selected Codex C06 CLI/TUI components. Organize `vcp-cli` as `args`, `commands`, `session_view`, `renderer`, `input`, `questions`, `jsonl`, `exit_status` and `workspace_resume`. Inspectors live in `vcp-audit` query services with terminal adapters here. CLI commands issue the same internal envelopes; terminal rendering never invokes workers directly.

Define presentation records for task/agent status, model/group/cost, pending questions, verification and artifact references. The UI reads immutable projections and uses cursors. It can show memory/routing capabilities as not ready during development, but must not fabricate a working release feature.

## P3-01 — Structured command surface

Implement run/task-file input, session list/resume/fork, pause/cancel, status, inspect and `--format jsonl`. Validate all flags before creating billable work. Keep stdout strictly versioned event/result JSONL and stderr diagnostics; process binary output goes through artifact references or a declared encoded content type.

Implement the architecture's exit codes: 0 completed, 1 internal failure, 2 invalid configuration, 3 failed verification/incomplete task, 4 required input, 5 budget exhausted, 6 cancelled, 7 unresolved effect, 8 durably paused. Persist richer state and return task reference; a forced process kill may not return a VCP code.

Tests: bad arguments, missing cap, invalid task file, noninteractive question, broken pipe, slow consumer, multiple events, cancellation and ambiguous effect. Parse stdout with an independent JSONL consumer; no progress prose or secrets may corrupt it. An early closed consumer invokes the owner-loss policy.

Build an argument parser that produces typed command inputs without starting
the engine/provider. Validate mutually exclusive flags, task-file decoding,
workspace selection, schema/output mode and persisted/explicit budget policy
before billable admission. Parameter errors must not leave orphan tasks or
reserve money. Keep shell argument parsing separate from task-file contents;
task text is task input, not a script to execute.

Define proposed JSONL envelope variants for command acceptance, durable event,
required input, cursor gap and final result. Each carries schema version,
correlation/task IDs and durable references as applicable. Emit exactly one JSON
value per line with consistent UTF-8 framing; partial terminal output and binary
tool bytes use declared content/artifact representations. A final result follows
the canonical result commit, not an optimistic UI status.

Implement the architecture's precedence explicitly: unresolved effect, then
cancellation, budget, required input, verification/incomplete, configuration and
internal outcome as applicable. Use code 8 only for a durably saved pause when
none of the higher-priority conditions applies. The final structured record
retains all conditions even when a single code summarizes them. A signal/forced
kill may have no VCP final line or code; callers recover via task identity.

Map `vcp tasks pause <task-id>` to the owning controller through qualified private
owner-control delivery or its structured input adapter. Authenticate the owner
identity and command scope and acknowledge the same durable pause command used
by `/pause`. Never create a second canonical writer. An unreachable owner yields
an explicit error; the public P9 attach server is not a first-release prerequisite.
Test malformed/stale owner identity and duplicate pause commands alongside the
same-process structured control path.

## P3-02 — Terminal workflow

1. Render objective, current step, model/group, known/reserved/uncertain cost, relevant changes and current-result checks. Support input while work is running through steering revisions.
2. Implement scoped questions with durable IDs and explicit answer/cancel handling. Do not submit a highlighted default without a user action.
3. Add command dispatch for `/pause`, `/resume`, `/cost`, `/memory`, `/history`, `/optimize` and `/agents` as services become ready. Missing services give a clear internal-stage state.
4. Handle resize, Unicode, narrow terminals, non-TTY output and long tool results. Sanitize terminal escape/control payloads in untrusted commentary/output; preserve raw artifacts separately.

Use a reducer over immutable projection/events to produce terminal view state.
The input handler creates internal commands; renderer code cannot reserve money,
execute tools or mutate canonical state. Keep input/cancellation queues bounded
and responsive independently of output rendering. Coalesce intermediate progress
updates where useful, while retaining durable events and final outcomes.

Make `/pause` visible and usable while work is active. It pauses root/child
scheduling and requests bounded cancellation/reconciliation, then keeps the CLI
open with the objective, paused state, partial changes, pending questions and
known/reserved/uncertain money. Status, history, cost and inspector navigation
remain available. `/resume` deliberately continues from that view in the same
process after revalidation; it does not require closing and reopening VCP.
Repeated `/pause` preserves the paused task, and submitting an answer while
paused cannot dispatch an effect implicitly.

Show pause separately from cancel and exit. A pause-in-progress view reports
still-running or unknown effects truthfully instead of claiming all processes
froze instantly. Steering entered while paused may update the task contract but
does not resume scheduling. Show whether guidance was queued, applied or blocked
by an already-running effect when steering an active task.

Test the renderer with fixed IDs/clocks for state properties, then test actual
Windows input/resize/close behavior. Include combining Unicode/wide characters,
long path names, malicious terminal escape sequences and tool-output floods.
Test that a question's highlighted default is never submitted by redraw, resize
or reconnect. Retain raw evidence while rendering sanitized text.

Tests use a pseudo-console where available plus actual Windows Terminal/manual cases for close and keyboard behavior. Assert meaningful event attribution and input responsiveness with bounded queues; avoid snapshots of incidental timestamps or exact model wording.

## P3-03 — Evidence inspectors

Implement query-to-view adapters for context manifests, actual serialized prompts, output artifacts, routing decisions/exclusions, policy origins, tool receipts, known/unknown cost, verification and canonical memory references. Show unavailable/redacted/truncated ranges explicitly. Page large results and load full content on demand rather than duplicating whole transcripts in terminal memory.

Early completion covers the common navigation/query contracts and existing state views. P5-06 later provides real memory search/evidence behavior; P3-05 provides retention UI. Inspector output uses current access/retention scope, including when viewing historical sessions.

Tests: navigate from action to authority, attempt, outcome and full output; inspect a pruned artifact; rebuild projections and repeat the query; deny another workspace's artifact ID. E17/U05 apply.

Define a shared proposed `InspectionPage` containing scope, source watermark,
view type, item/range references, visibility/gap markers and next cursor. Query
services enforce current access before loading content and before returning each
artifact range; knowing an artifact ID is insufficient. Page ordering and filters
belong in the query contract rather than terminal-specific sort code.

Implement one navigable evidence chain first: task → serialized request and
context manifest → attempt/reservation → tool proposal and policy → dispatch and
outcome → verification. Show requested and served model identities separately,
and raw versus normalized usage where useful. Expose instruction/path provenance
and omission reasons beside selected context. The exact-request view must say
when bytes are redacted, missing or only reconstructed.

Use bounded range reads for large outputs. A pruned range returns an explicit
retention marker and source identity, not empty content that looks like a tool
returned nothing. Rebuild projections in a fixture and compare inspectable
relationships at the same canonical watermark. Later memory/history views use
these same pagination/access contracts rather than maintaining another transcript
database.

## P3-04 — Workspace continuation

Starting `vcp` resolves durable workspace identity, then summarizes unfinished tasks. A single resumable task offers one-step continuation; multiple tasks use a chooser. `vcp resume --last` is explicit automation selection, not an automatic replay on startup.

Display paused children, partial changes, unsettled money and unknown effects before resuming. Revalidate files/root bindings and current authority. Imported machine state cannot carry blanket local grants. A missing root or invalid worktree produces a rebind/reconcile action while preserving history.

Test clean exit, hard close, multiple paused tasks, moved root, deleted worktree, changed AGENTS.md and partial child result. U06 verifies reopening discovers context without silently dispatching an old non-idempotent effect.

Implement discovery as read/reconcile before continuation. Resolve the durable
workspace through verified repository/worktree bindings, scan unfinished task
projections, and load enough current recovery/accounting state to explain each
candidate. A single candidate provides a one-step resume action but does not
automatically run it. Stable ordering for multiple candidates should be explicit
(for example last activity with an ID tie break); `--last` is an explicit selector,
not authority to bypass reconciliation.

The same resume handler serves `/resume` in an open paused CLI and workspace
reopening. Bind the selection to task and expected state revision so a changed
task cannot be resumed through a stale chooser row. Revalidate root/instruction
and policy state and show unresolved effects before dependent mutations. A
missing root requires explicit rebind/reconcile without erasing historical
paths; an imported snapshot requires destination grants.

Add a same-process acceptance path: start deterministic work, `/pause`, inspect
history/status/cost, alter an input file externally, then `/resume`. Assert the
CLI stayed open, no work was scheduled while paused, changed input invalidated
stale preparation and the resumed attempt used current instructions/budget.
Repeat with paused children and pending input. Keep this separate from console
close, crash and new-process restoration evidence.

The chooser/child view shows whether a pause came from the parent or an explicit
child decision. Parent resume must preserve independent child pause/cancellation;
do not present every child as resumed merely because the root changed state.

## Exit and handoff

Run `cli`, `context`, `recovery` and E01/E09/E17 internal-client cases. Record CLI/headless parity traces, real Windows console cases and any not-run terminal environments. P3-01–04 complete when users can control, inspect and resume the same durable engine from either CLI mode. Public API/editor support remains deferred.
