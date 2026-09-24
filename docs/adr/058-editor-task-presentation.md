# ADR-058: Access-checked task presentation and editor actions

Date: 2026-09-23
Status: implemented and qualified under P4-02; [acceptance evidence](../development/editor-tasks.md#qualification).

## Decision

The editor task/child view uses the existing canonical snapshot and ordered public
events. Public events are invalidation and evidence metadata, not complete task
reducers. An event invalidates affected presentation; a fresh engine read supplies
current state. A gap requires resynchronization. A closed transport, local spinner
or accepted command never proves task completion.

Add a separate bounded `task/presentation` read rather than extending the existing
strict task/event shapes. The engine projects objective, retained assignment/model
information, ledger cost, approval summaries and attributed evidence. Missing
retained information is explicitly unavailable; configuration or model policy is
not evidence of the model that executed a request. Each page rechecks current
access and retention. The extension does not deserialize private canonical facts.

The host registers opaque action IDs against current task/input revisions. Webview
messages select only those actions. Steering text is collected by the extension
host, and links select authorized artifact references rather than arbitrary URIs.
Model/tool text is rendered as text with constrained local resources, without HTML
execution or implicit command links. Each child retains its own status and bounded
commentary/evidence window so a noisy child cannot hide another waiting child.

Before submitting a mutation, persist its durable command ID and minimal scope and
operation metadata. Never persist its text, credentials or approval payload in
webview/workspace state. Duplicate clicks share the pending operation. Reload or
timeout queries `command/read` using the same ID and refreshes canonical state;
it never generates a replacement or automatically replays the operation. An
unavailable receipt leaves an unknown outcome because absent and pruned evidence
cannot prove that the command did not execute. Acceptance means acceptance, not
successful completion.

Controller ownership and editor trust are checked in the extension and enforced
again by the engine. An observer cannot answer a question or acquire control via
a view message. Approval decisions use the current input, effect, policy and
steering revisions. Answering does not resume paused work; resume is deliberate.

## Qualification

P4-02 requires snapshot/event parity with the CLI's canonical semantic projection,
cursor-gap recovery, independent child bounds, duplicate/stale actions, unknown
command outcomes and actual editor interaction with hostile markup. P4-01's
connection qualification remains separate. Versioned edits, inspectors and
distribution retain their P4-03, P4-04 and P4-05 ownership.
