# Version 1 lifecycle hooks

P10-01 adds an opt-in trusted-owner API in `vcp-lifecycle::foundation::hooks`.
Executable hook configuration is distinct from skill discovery and imported
configuration. No repository instruction, skill, model response or restored
history activates a hook. Configure the ordinary native process profile first;
its executable, script inputs, environment, process-count and isolation ceilings
continue to apply. Windows is the qualified executable host for this increment.

The version 1 events are `session_start`, `task_start`,
`before_context_assembly`, `before_tool_authorization`, `after_tool_completion`,
`before_compaction`, `after_verification` and `task_completion`. The owner supplies
an event projection through `hook_input`; the adapter binds canonical workspace,
session, task, root task and current steering. Artifact references must be complete,
readable, retained artifacts of that task. Only the explicit payload is disclosed;
the adapter never supplies ambient conversation, provider credentials or memory.

`vcp-extensions::hooks` owns the versioned input, registry, planner, wire, result
and receipt schemas. Hook configuration records ID, version, source hash, event,
priority, before/after constraints, profile/arguments/directory, broker effect
scope, timeout, output limit and failure policy. The source hash is configured
provenance; the broker additionally pins and records the actual executable and
profile input versions. Changing configured provenance cannot authorize changed
executable bytes.

Planning is pure. Ready nodes sort by priority then stable hook identity, with
declared before/after edges taking precedence. Duplicate identities, missing
ordering targets, cross-event edges and cycles reject the registry. Default
limits are eight trigger levels, 32 hooks per event and 64 KiB of input. Hard
ceilings prevent callers from turning configurable bounds into unlimited work.
Hooks are sequential; executing a hook does not itself deliver another lifecycle
event. An owner emitting a causal child event must propagate its causation and
increment depth. Hook programs have no lifecycle-admission API.

## Execution and authority

The explicit trusted CLI profile accepts an optional `hooks` array of version 1
definitions. Each command must name one of that profile's `processes`; profiles
remain outside repositories and sync roots. Installation validates the registry
before the owner submits a turn. No hook config is inferred from a project file.

The shared CLI execution owner delivers `session_start` and `task_start` before
submission, and gates canonical completion with `task_completion`. The retained
HTTP adapter awaits `before_context_assembly` before each newly admitted canonical
model context. `before_compaction` runs only when the configured pure portable
compactor has an eligible projection and the history has not already failed its
gain check; final serialized gain and source validation still govern publication.
Native file/process wrappers deliver before-authorization and after-completion
events; verification delivers `after_verification` and `after_tool_completion`.
Version 1 authorization/rewrite gates support native file and process requests
only. When any `before_tool_authorization` hook is configured, the retained
`vcp_mcp` and `vcp_verify` wrappers reject the request before dispatch rather than
silently bypassing that gate. A configuration needing verification should use
the supported lifecycle validation events until verification authorization hooks
are qualified. Direct trusted library callers use the explicit owner adapters.

Without before-authorization hooks, completed MCP operations also deliver the
configured after-completion event. An active MCP connection owns the process
broker, so an executable completion hook cannot run alongside it: the wrapper
reports a blocked notification and pauses the affected task. The completed MCP
effect is retained; this does not roll it back or authorize automatic replay.

Session/task/context findings and warnings are captured as receipt-linked
`Untrusted` evidence parts in canonical context. A bounded map deduplicates by
receipt artifact, including after an explicit owner resume; those parts never
enter lifecycle event identity or become operating instructions. Completion
findings and warnings are presented by the CLI with terminal sanitization.
An accounted transport retry reuses its existing permit and cannot rerun hooks.
Explicit CLI approval resume may create a fresh canonical turn; lifecycle
identity uses the captured user-input digest and current history/authority,
preserving the original pending hook when only the turn correlation ID changes.
Changed input or steering selects a different operation under fresh authority.
Asynchronous model preparation arms a local boundary token. Synchronous context
assembly consumes it and checks the current input and every canonical hook receipt;
an intervening steering or source change rejects admission. Only an already
accounted transport retry can reuse the accepted token, with the same checks.

Version 1 requires a direct, nonterminal executable profile. The broker supplies
one canonical JSON document on stdin, followed by LF. It appends
`--vcp-hook-input-sha256 <digest>` to the configured literal argument list; the
digest covers the document bytes without LF. Thus process approval binds the
complete planned input, not merely the executable name. Environment values come
only from the broker's public bootstrap allowlist. Ambient secrets are not
inherited. `broker_profile` scope means the ordinary governed opaque process
scope, not a claim of additional filesystem or network confinement. Unsupported
required isolation still rejects dispatch.

The executable must return one strict version 1 JSON object on stdout:

```json
{"schema_version":1,"findings":[],"context":null,"rewrite":null,"block":false}
```

Unknown fields, unsupported schema versions, malformed JSON and unauthorized
artifact references block the action. Context has `text` and `artifact_refs`;
it is attributed external input, never system instructions or memory acceptance.
A rewrite has `original_digest`, `tool` and object-valued `arguments`; it is legal
only before tool authorization and must match the input's `operation_digest`.
A blocked result cannot also propose context or a rewrite. Hooks cannot accept
memory, grant permissions or alter budget balances.

`prepare_hook` produces a one-use live proposal, exposing its current policy
decision and approval question. `dispatch_hook` uses the ordinary process broker
and rechecks current scope, native versions, authorization, pause generation and
owner. `run_hooks` executes an ordered batch, retaining approval-pending proposals
only for the current owner. Successful duplicate deliveries reuse their durable
decision with `duplicate: true`; consumers must not publish context twice.

`rewrite_hook_process` and `rewrite_hook_tool` consume and cancel the original
proposal, parse the full new request through the existing schemas, revalidate
resources, and prepare a new operation under current policy. Old approvals do not
transfer to changed paths, arguments or shell fragments. Rewrites cannot replace
the process kind with a different tool. Multiple rewrite proposals for the same
immutable input are ambiguous and block the adapter rather than silently choosing
one. The owner can inspect the proposals and submit a new event explicitly.
The raw tool adapters revalidate the complete hook batch in the same canonical
worker operation that proposes final authority, so a later hook cannot silently
invalidate an earlier rewrite's source and still publish it.

## Failure and recovery

The deterministic `hook-plan-<identity>` artifact records input and ordinary
effect ID before any launch. Its creation is serialized by the canonical worker.
The existing broker records dispatch intent before executing; its effect journal
and recovery rules remain authoritative. After execution, validated output is
committed as `hook-result-<identity>` before any proposal is returned for use.
Both file and SQLite stores use these same artifact and effect contracts.

Duplicate admission never launches another executable. After restart, a plan
without a result is inspection/reconciliation work, even if the process journal reports
successful exit. Lost owners, cancelled futures and unknown effects cannot cause
automatic reruns. `inspect_hook` remains available while paused and reports the
plan, ordinary effect state and optional receipt. Restart does not reconstruct a
live approval capability from serialized input. An explicit new event is a new
operation, still subject to the workspace's unresolved-effect fence.

Use the interactive terminal or local service when hooks may require approval.
Those owners retain the live pending capability while the user answers and
explicitly resumes. Batch `--control-stdin` remains a stop-only protocol. Ordinary
one-shot batch execution retains its existing needs-input exit behavior;
reopening it does not reconstruct a
pending hook capability. Inspect/reconcile the recorded intent and explicitly
steer or configure a fresh versioned event if another attempt is intended.

Security and validation failures block. Gate-event execution failures, including
timeouts, block. Only notification events may explicitly select warning policy;
those failures produce durable, visible warning receipts. Parent pause prevents
new dispatch. Late output is retained as blocked historical evidence when current
scope/steering no longer validates, and cannot authorize a rewrite.

The configured source hash identifies the declared hook version. Independently,
the reservation binds the broker profile, executable file version, pinned script
inputs, working-directory identity and admitting authority. Cached results must
pass those checks again; changing source bytes or policy cannot silently reuse a
prior result. Approval-pending capabilities are bounded and retained only in the
current owner; superseded authority cancels them.
After explicit owner pause/resume, a live approved proposal may acquire the
current scheduling generation only while its effect remains validated with no
execution identity. Scope, owner, native resource versions and current policy
must still match. This does not reconstruct a proposal from history or reset a
dispatched effect.

The upstream comparison is deliberately separate: see the
[G04 qualification](../evaluations/p10-01-gemini-hooks.md) and
[intentional differences](p10-hooks-upstream.md). Gemini configuration parsing,
regex matchers, ambient hook environments, permissive output and automatic
failure continuation are not compatibility promises.
