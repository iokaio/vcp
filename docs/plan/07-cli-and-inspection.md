# 07 — Interactive CLI, structured output and inspectors

Status: planned. Owns P3-01 through P3-04. Begin after P2 verification/recovery/continuity. Memory/history/pruning and vault UI extensions are owned by segments 09–11, not duplicate work here.

## Code organization

Adapt selected Codex C06 CLI/TUI components. Organize `vcp-cli` as `args`, `commands`, `session_view`, `renderer`, `input`, `questions`, `jsonl`, `exit_status` and `workspace_resume`. Inspectors live in `vcp-audit` query services with terminal adapters here. CLI commands issue the same internal envelopes; terminal rendering never invokes workers directly.

Define presentation records for task/agent status, model/group/cost, pending questions, verification and artifact references. The UI reads immutable projections and uses cursors. It can show memory/routing capabilities as not ready during development, but must not fabricate a working release feature.

## P3-01 — Structured command surface

Implement run/task-file input, session list/resume/fork, pause/cancel, status, inspect and `--format jsonl`. Validate all flags before creating billable work. Keep stdout strictly versioned event/result JSONL and stderr diagnostics; process binary output goes through artifact references or a declared encoded content type.

Implement the architecture's exit codes: 0 completed, 1 internal failure, 2 invalid configuration, 3 failed verification/incomplete task, 4 required input, 5 budget exhausted, 6 cancelled, 7 unresolved effect, 8 durably paused. Persist richer state and return task reference; a forced process kill may not return a VCP code.

Tests: bad arguments, missing cap, invalid task file, noninteractive question, broken pipe, slow consumer, multiple events, cancellation and ambiguous effect. Parse stdout with an independent JSONL consumer; no progress prose or secrets may corrupt it. An early closed consumer invokes the owner-loss policy.

## P3-02 — Terminal workflow

1. Render objective, current step, model/group, known/reserved/uncertain cost, relevant changes and current-result checks. Support input while work is running through steering revisions.
2. Implement scoped questions with durable IDs and explicit answer/cancel handling. Do not submit a highlighted default without a user action.
3. Add command dispatch for `/pause`, `/resume`, `/cost`, `/memory`, `/history`, `/optimize` and `/agents` as services become ready. Missing services give a clear internal-stage state.
4. Handle resize, Unicode, narrow terminals, non-TTY output and long tool results. Sanitize terminal escape/control payloads in untrusted commentary/output; preserve raw artifacts separately.

Tests use a pseudo-console where available plus actual Windows Terminal/manual cases for close and keyboard behavior. Assert meaningful event attribution and input responsiveness with bounded queues; avoid snapshots of incidental timestamps or exact model wording.

## P3-03 — Evidence inspectors

Implement query-to-view adapters for context manifests, actual serialized prompts, output artifacts, routing decisions/exclusions, policy origins, tool receipts, known/unknown cost, verification and canonical memory references. Show unavailable/redacted/truncated ranges explicitly. Page large results and load full content on demand rather than duplicating whole transcripts in terminal memory.

Early completion covers the common navigation/query contracts and existing state views. P5-06 later provides real memory search/evidence behavior; P3-05 provides retention UI. Inspector output uses current access/retention scope, including when viewing historical sessions.

Tests: navigate from action to authority, attempt, outcome and full output; inspect a pruned artifact; rebuild projections and repeat the query; deny another workspace's artifact ID. E17/U05 apply.

## P3-04 — Workspace continuation

Starting `vcp` resolves durable workspace identity, then summarizes unfinished tasks. A single resumable task offers one-step continuation; multiple tasks use a chooser. `vcp resume --last` is explicit automation selection, not an automatic replay on startup.

Display paused children, partial changes, unsettled money and unknown effects before resuming. Revalidate files/root bindings and current authority. Imported machine state cannot carry blanket local grants. A missing root or invalid worktree produces a rebind/reconcile action while preserving history.

Test clean exit, hard close, multiple paused tasks, moved root, deleted worktree, changed AGENTS.md and partial child result. U06 verifies reopening discovers context without silently dispatching an old non-idempotent effect.

## Exit and handoff

Run `cli`, `context`, `recovery` and E01/E09/E17 internal-client cases. Record CLI/headless parity traces, real Windows console cases and any not-run terminal environments. P3-01–04 complete when users can control, inspect and resume the same durable engine from either CLI mode. Public API/editor support remains deferred.
