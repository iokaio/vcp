# ADR-064 — Governed editor inspector views

Date: 2026-09-24

Status: accepted for the qualified Windows and VS Code 1.138.0 envelope.

## Context

P4-04's query, optimizer and encrypted publisher APIs are qualified separately.
An editor view must preserve their current access checks through paging, task
changes, hidden views, reload and uncertain mutation outcomes.

## Decision

Use one extension-host inspector session with nine fixed categories: history,
memory, evidence, cost, policy, routing, optimizer, pruning and publisher. Read
one bounded engine page or artifact range at a time. Keep a bounded transient
navigation stack, and reauthorize every navigation and refresh through the SDK.
Do not infer current permission from retained grants or historical evidence.

The webview sends only fixed navigation messages and host-issued opaque action
IDs. It receives inert text and exact decimal counters, never executable links,
provider credentials, recovery material or command arguments. It has no persisted
content state. Hiding, recreating or revealing it clears the presentation. Task
selection, stream loss, connection, trust and binding changes invalidate transient
arguments; late replies cannot populate a replacement context. A periodic fresh
read bounds retention of content when an invalidation event is unavailable.

Reject an entire oversized presentation and its action handles instead of
truncating an actionable policy review. Distinguish historical observations,
current authorized reads, missing data, partial results and independent backup
publication, cleanup, cloud-transfer and restore facts.

Optimizer and pruning previews remain ephemeral. Pruning preview creation and
paging are authorized reads available to observers; their cache belongs to the
creating connection. Applying pruning still requires controller authority. Host
prompts collect bounded inputs and confirmation; mutation dispatch retains the
exact engine preview identity and revisions. Encrypted publication requires explicit controller
profile selection through the native loader. Observer recovery never selects or
loads publisher credentials.

Before sending a durable mutation, persist its unique command ID with only
nonsecret method, scope, target and receipt metadata. Partition writers by engine
and data profile and retain a single writer when navigating between profiles.
Reload reconciles these IDs with `command/read`; it never resubmits a mutation.
Late authenticated acceptance remains attached to its original journal even when
the current view changed. Missing or pruned receipts cannot erase known acceptance.

## Qualification

Portable tests cover the strict message boundary, inert hostile text, exact large
counters, display bounds, stale replies, task and profile changes, hidden-view
clearing, artifact range identity, command persistence and explicit mutation
guards. Actual installed-editor qualification passed on Files and SQLite; see the
[inspector acceptance record](../development/editor-inspectors.md).

The native test harness may opt into an ephemeral loopback Chromium debugging
port in its private editor profile. It clicks the actual renderer and observes
DOM results. This option belongs to the qualification launcher, not the extension
or its user configuration. It must not attach to an existing user editor.

## Consequences

Views reread data after visibility and authorization changes. An interrupted
review must be recreated; it is not recovered from storage. Unknown submitted
commands remain inspectable without risking duplicate effects. Packaging and
large-history startup qualification remain owned by P4-05.
