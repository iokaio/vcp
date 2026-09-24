# ADR-065 — Governed lifecycle hooks

Status: selected for P10-01; qualification evidence is tracked separately.

## Decision

Version 1 planning and output schemas live in `vcp-extensions::hooks`. The
canonical lifecycle adapter executes direct nonterminal profiles through the
existing process broker. There is no second authority store or effect journal.

The planner orders priorities and stable identities under explicit dependency
edges. Duplicate identities, unresolved/cross-event edges and cycles reject
configuration. Input binds event/source identities, canonical scope and steering,
causation, permitted artifacts and bounded projections. The process arguments
bind a digest of the full stdin document: approval cannot transfer to changed input.

One deterministic canonical artifact reserves each delivery before dispatch.
The ordinary broker journals the effect. A second artifact records validated
output before publication. A reservation without a result requires reconciliation,
even after successful process exit. Restart and duplicate delivery never
automatically repeat uncertain effects.

Output is untrusted attributed data. Native-tool wrapper gates run before final
approval. Source drift without a rewrite requires refreshed hook input. Rewrites
use existing tool schemas, refresh resource/instruction scope and receive fresh
policy evaluation. Explicitly rewriting an approved proposal consumes/cancels
the old proposal. Multiple rewrites of one immutable input block as ambiguous.

Gate failures block; configured notification failures produce visible warnings.
Malformed output always blocks. Late output must pass current steering/scope
checks. After-action blocking pauses further work without claiming the completed
effect was undone. Pause and owner loss retain existing admission barriers.

## Alternatives and limits

Pinned Gemini G04 suites provide separate behavioral evidence. VCP rejects shell
expansion, ambient environment overrides, unbounded output, permissive malformed
JSON success and automatic failure continuation. The
[comparison](../development/p10-hooks-upstream.md) records ADR-014 differences;
this decision adds no foreign wire/configuration compatibility.

The owner API covers eight reserved events. The retained native-tool wrapper
connects before-tool authorization, after-tool completion and after-verification.
The shared CLI execution owner runs session/task start before submission and task
completion before canonical finalization. The retained request path awaits the
new default asynchronous `HostWorkAdmission::prepare_model` seam before its
existing synchronous admission. VCP uses that seam for context assembly and an
eligible portable-compaction projection; it never blocks the synchronous store
worker awaiting an executable. Canonical admission still owns the transport grant.
It consumes a current hook-gate token and rechecks the event boundary and source
fences; finishing asynchronous preparation alone cannot grant a later request.
Accounted transport retries reuse their permit without repeating hook execution.
Context event identity binds captured user-input bytes, current history and
authority revisions, excluding transient turn IDs and task-state revisions so
explicit approval resume can continue the original pending hook safely.
Registrations require explicit owner setup, are absent after restart and cannot
be replaced during a thread. Broker profiles retain their actual opaque-effect
and isolation limits; no new OS sandbox is claimed.

Start and completion hooks run as bounded owner jobs without claiming another
retained event receiver. Terminal input, deadlines and local control remain live;
submitted turn IDs fence delayed events and cleanup fences execution before
draining jobs. Interactive/local owners can retain a pending approval capability.
One-shot batch retains its existing needs-input exit and stop-only stdin protocol;
reopening never reconstructs a hook capability from history.

Version 1 authorization rewrites support native file and process operations.
Configured authorization hooks reject unsupported MCP and verification wrapper
paths explicitly; no specialized tool may silently bypass the configured gate.

Context is receipt-linked input, not operating instructions, accepted memory or
budget mutation. Hook programs cannot mint VCP inference authority. Native effects
remain visible in ordinary history; external service charges inside an opaque
executable are not claimed to be metered by VCP.

## Evidence and reconsideration

The [hook contract](../development/hooks.md) defines schemas and APIs.
`scripts/test-hooks.ps1` exercises pure contracts, adapters and native both-store
marker/owner-kill cases. [G04 evidence](../evaluations/p10-01-gemini-hooks.md)
records the pinned upstream subset independently. Revisit this decision before
adding executable hosts, shell hooks, automatic imports, parallel rewrites or new
effect/isolation guarantees.
