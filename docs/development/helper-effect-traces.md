# Review and compaction effect map

This is a P0-07 map of the retained Codex baseline at
`3d3ae4965ab370217e871b3a7f0d15589557ee4b`. The
[nine-case native trace](native-cli-trace.md) observes representative helper
requests; [the boundary catalog](../../src/third_party/components/codex-boundaries.json)
checks source anchors. These are discovery and qualification inputs for
P0-03/P0-08. They do not install VCP adapters or qualify all upstream effects.

Paths below are relative to
[`src/third_party/codex/codex-rs/`](../../src/third_party/codex/codex-rs/).
Classification applies to the named entry, including effects it invokes. A
protocol value carried by an effectful entry is not itself authority.

| Entry | Classification and observed or source-visible behavior | VCP implementation responsibility |
|---|---|---|
| `exec/src/lib.rs::run_main` | Effectful: loads configuration, starts an in-process app-server client, submits `turn/start` or `review/start`, consumes notifications and shuts down the client | P0-03 retains one owner, injects workspace/task/authority identity before submission, and carries lifecycle controls through this private path |
| `core/src/session/review.rs::spawn_review_thread` | Effectful: resolves review model metadata, creates review context, configures tools and starts a review task; metadata refresh is another potential network surface | P0-08 admits discovery and helper requests through the configured VCP gateway; model choice does not authorize a provider |
| `core/src/tasks/review.rs::start_review_conversation` | Effectful: constructs a constrained child configuration and invokes `codex_delegate::run_codex_thread_one_shot`; the native observer sees one model request carrying the review rubric | P0-03 keeps the child inside root pause/cancellation authority; P0-08 attributes each request and usage receipt to root and child |
| `core/src/tasks/review.rs::process_review_events` | Effectful: consumes child events, forwards selected events and derives structured/fallback review output; the successful native fixture reports zero usage at the parent despite provider usage | Preserve complete request receipts independently of the parent display. Never infer zero cost from missing/zero parent usage |
| `core/src/session/turn.rs::run_auto_compact` | Effectful: chooses token-budget rollover, remote compaction or the Responses-based fallback according to feature/provider capabilities | P0-08 fences every alternative; configuring the main provider URL is insufficient to remove alternative routes |
| `core/src/compact.rs::run_inline_auto_compact_task` | Effectful: synthesizes a summarization prompt and makes another model request. In the tested fallback, the second request has no tools and consumes the rejected tool receipt | Admit compaction with an explicit helper purpose, authority revision and budget reservation. Upstream's "local" compaction label does not mean local inference |
| `core/src/compact.rs::run_compact_task_inner_impl` | Effectful: updates compacted prompt history and emits compaction lifecycle events; the third request consumes the summary and no longer includes the tool receipt | Store observations and receipts durably before projecting compacted context. A summary cannot replace canonical history, provenance or unresolved effects |
| `exec/src/lib.rs::request_shutdown` | Effectful: sends `thread/unsubscribe`; `run_main` subsequently shuts down the client. Native traces observe process exit and no extra loopback request | P0-03 must implement durable root/child admission fencing and reconciliation. Normal exit is not a close/reopen, crash or `/pause` test |

The review and compaction denied cases each inject HTTP 401 at the helper
request. Both produce a failed turn and nonzero native exit. Compaction denial
must not generate the next coding request. Review denial can emit an interrupted
review explanation; that explanation is not a successful review result.

## Adapter sequence and acceptance

Use [the engine execution design](../architecture/engine-execution-design.md)
and [context/provider design](../architecture/context-provider-design.md) as
the owning contracts. First identify each request before dispatch with stable
root/task/attempt IDs, purpose and originating authority revision. Reserve
budget atomically at the shared gateway, then persist the observed response and
usage independently of the CLI projection. A retry is another observed attempt;
a transport failure cannot establish that no provider-side work occurred.

Record prepared tool decisions and effects in canonical history before allowing
compaction to replace the model-facing conversation. Link a summary to its
source revision and retained observations; scope filtering and deletion must
still apply when the summary is used. The fixture intentionally proves that
upstream prompt history drops a tool receipt, so treating that history as VCP's
only durable evidence would violate the contract.

For `/pause`, fence new root, child and helper admission before acknowledging the
pause. Keep the CLI open for inspection, retain independently paused children,
and prevent stale completions from authorizing further work. Retain late
observations for reconciliation instead of discarding their possible effects.
Exercise pause during the review
stream, between tool receipt and compaction dispatch, and between compaction
completion and the next coding request. Resume only after deliberate
revalidation and effect reconciliation; closing the owner uses the same
boundary. These are required P0-03/P0-08 tests, not results of this baseline.

Extend the observer for background memory, guardian, WebSocket/prewarm,
provider-specific compaction and credential/discovery routes as their adapters
are inserted. The current fixture disables background memory and never enables
realtime, MCP or executable hooks. It observes loopback HTTP only and provides
no OS-wide network-denial proof. Complete module effect qualification remains
open in [plan 01](../plan/01-upstream-feasibility.md).
