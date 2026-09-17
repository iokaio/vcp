# 06 — Windows execution, policy, prepared edits and recovery

Status: planned. Owns P2-03, P2-04 and P2-07. P2-03 depends on domain/store and the P0 Windows/reuse decisions; P2-07 also requires the session loop and projections. Architecture sections 9, 10 and 14 govern effects.

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

Tests: deny versus allow precedence, command rewrites after approval, changed MCP schema, redirection/argument ambiguity, new path outside grant, expiration and repeated confirmation. Assert actual dispatch counts at the broker. E06/R03 apply.

## P2-04 — Tools and Windows worker

1. Implement read/list/search tools with canonical root checks and bounded output. Escape untrusted terminal control sequences at presentation; keep underlying captured bytes attributed.
2. Adapt patch parsing into an edit plan. Read expected versions, preserve CRLF/encoding and staged/unstaged/untracked state, then revalidate immediately before each write. Detect rename/delete/case and link changes. Never use a rollback that overwrites subsequent human edits.
3. Record multi-file outcomes individually. Where atomic multi-file application is unavailable, report partial effects and keep recovery receipts; avoid promising filesystem compare-and-swap stronger than the tested primitive.
4. Reuse native Windows process/PTY/job controls with explicit executable, argument vector, working directory, inherited handles and filtered environment. Secret handles are scoped; the child does not inherit all engine credentials.
5. Bound process count, execution time and output spooling. Cancel process trees and report residual effects independently of exit status. Git/publishing/network tools use the same policy path as file effects.

Tests use real temporary Windows roots and helper executables: spaces/Unicode, quoting/metacharacters, CRLF, locked file, long path, directory replacement, junction escape/race, stale file hash, partial rename failure, noisy child and surviving grandchild. Verify actual outside-root markers remain untouched, edited bytes match intent, and test reports state which OS controls were enforced. E05/E07/E08/R02/R04/R05 apply.

## P2-07 — Pause and unknown-effect reconciliation

Use the following durable order: prepared/authorized operation, dispatch intent commit, worker dispatch, observed outcome, outcome commit. Place fault-injection barriers before/after every step. Only supported idempotent/reconciled operations may be retried; a missing success record is not permission to repeat an effect.

1. On close, explicit pause or controlling-client loss, mark stopping, block new model/tool/maintenance/child scheduling and request bounded cancellation. Persist partial artifacts and charge uncertainty.
2. Periodically checkpoint during work; forced process termination cannot depend on a graceful exit hook. Test actual console closure separately from synthetic connection loss.
3. On restart, inspect durable intent and owned process identity, actual file versions/receipts and available provider status. Guard against PID reuse before process operations. Classify outcomes as known, reconciled or unknown.
4. Reconstruct paused root/child state and explain required decisions. Resume revalidates current root bindings, policy, instructions, files, model capabilities and available budget.

Tests: kill before dispatch, after external effect but before receipt, during output write, during reservation settlement and during pause checkpoint. A non-idempotent marker fixture records the number of actual effects outside VCP state. On reopen it must never gain a second marker solely from automatic replay. Unknown usage remains reserved. U06/E08/E10/E12 apply.

## Exit

Run `tools`, `windows`, `recovery` and relevant `store` conformance. Evidence includes policy/dispatch receipts, process-tree observations, current file fingerprints and post-reopen state. Any silent overwrite, unauthorized effect, hidden surviving scheduler or replayed uncertain effect blocks completion.
