# Completed P2 integration boundaries

The [P2 acceptance record](../evaluations/p2-completion.md) is the current phase
status. Earlier increment guides describe the boundaries at the time they were
implemented; their pending integration notes are superseded by that record.

## Provider retries

The retained HTTP client reports normalized failures to the canonical host.
Implicit library and authentication-recovery retries remain disabled for hosted
requests. The host permits at most two retries, with bounded exponential backoff
and an absolute deadline shared by the chain. Unsupported or excessive
`Retry-After` values stop the chain. Authentication, capability and protocol
failures do not retry. Incomplete streamed responses do not replay.

Each retry captures its actual request, obtains a new reservation and attempt,
and records its predecessor. Ambiguous earlier attempts retain independent
liabilities. A positively local capture failure before submission releases its
reservation. Pause, owner loss, steering, stale sources, exhausted budget and
deadline expiry prevent another dispatch. Coding retries reassemble current
state and cost uncertainty before obtaining fresh admission.

## Tool scheduling and completion

`CanonicalHost::schedule_tool` and `schedule_process` use the immutable prepared
operation's resources. Shared reads and disjoint effects can proceed; overlapping
writes and opaque commands conflict. Limits bound active work, work per task,
queue depth and queue wait. Conflicting waiters retain ordering while independent
results preserve their original call IDs. Windows case and ambiguous non-ASCII
paths use conservative conflicts.

Admission generations and absolute deadlines fence queued callbacks. Native
process and PTY launch check the same generation under the lifecycle lock through OS dispatch. An
unfinished process handle fences its owner before releasing resource claims.
Failed startup observation also fences the owner before a claim is released.
Neither a provider parallel-call hint nor a saved proposal grants concurrency.

An empty response without completed calls or observed assistant text cannot
finish a coding turn. Root request/output/deadline bounds also apply to failed
repairs. The host exposes memory as not ready and routing as a fixed qualified
model; later phases supply those capabilities. Current verification remains the
only completion path for edited code.

## Recovery and native close

`CanonicalHost::reconcile_effects` observes pending canonical effects while
paused; reopen performs the same scan. File plans are compared with current
native identities and bytes. Reports distinguish applied, unapplied, partial and
conflicting state, without claiming matching bytes prove authorship. Matching
execution receipts can settle an observed process outcome. A stale PID never
authorizes signalling or replay. Missing or contradictory observations retain
`OutcomeUnknown` and block dependent work.

The owning console installs `install_console_close_handler` and retains its
guard. A real Windows close event fences admission, requests bounded cancellation
and checkpoints partial evidence. Forced termination instead relies on durable
dispatch records and reopen. Resume checks current root/instruction observations,
configured model catalog, unresolved effects, policy and remaining budget; it
does not resume a saved native stack or recreate execution profiles.

The canonical worker awaits backend shutdown before releasing store ownership.
SQLite close waits for its native worker to terminate, so an immediate reopen
does not race residual database locks. Direct store users requiring this ordering
await `Store::close`; ordinary connection drop is not that acknowledgement.

## Handoff and context continuity

Every outgoing coding context captures `canonical-context-handoff/1`. The packet
contains the exact selected parts and revisions, current task/effect/question
state, available verification/base observations, the root ledger and remaining
funds after protected, active and uncertain liabilities. Original references
include compacted tool pairs and raw provider responses.

Provider-native response state is explicitly excluded from portable context and
recorded by artifact/hash/reason. Known opaque fields additionally retain their
paths and hashes. Their contents are not fabricated into reasoning or promoted
to instructions. `vcp_context::handoff::Packet::reassemble` checks current
revisions, ledger and every referenced artifact, then fits the destination's
envelope with normal complete-pair and trust checks. Incompatible role/tool
semantics fail visibly. The returned seal still requires host admission and a
fresh reservation; grouped model switching remains P6.

P2's native terminal profile supports bounded initial input bound to its prepared
operation. Follow-up input/resizing is not exposed as an ungoverned control.
Installed interactive CLI behavior remains P3. The qualified native execution
boundary remains explicit reduced isolation, not a general filesystem/network
sandbox.
