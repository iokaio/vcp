# ADR-061: Observed policy and routing inspection

Date: 2026-09-23
Status: accepted query prerequisite under P4-04; full inspector acceptance remains open.

## Decision

`policy/read` and `routing/status` expose bounded, read-only observations through
the authenticated lifecycle connection. Independent `policy/inspection/1` and
`routing/status/1` profiles require actual hosted methods and explicit negotiation.
The requesting actor and current session/task scope remain the authorization
boundary. An inspector never substitutes the host owner's credentials.

Policy inspection distinguishes stored workspace policy, observed task-effective
constraints and grant provenance. Shared grant identities remain unavailable to
scoped observers. Exact task grants and actually authorized inherited grants may
be presented with their original revisions, expiry and revocation state. A grant
row is not dispatch permission: actual operations still require the existing
current ownership, trust, prepared arguments, resources, isolation, skill and
child-scope checks. Missing live child-binding evidence is explicit unavailability,
not a reconstructed capability.

Routing inspection distinguishes the persisted policy from the effective policy
derived under this host's configured ceilings. A reopened host without explicit
routing configuration does not invent effective limits. Registry summaries retain
source identity and current source access/retention checks. Metadata-only source
observations do not attest retained-byte integrity; authorized artifact reads
perform their existing verification before returning content.

Every page rechecks current access. Continuations bind actor, scope, query and
relevant canonical/configuration revisions. Responses use exact decimal counters,
bounded text and explicit unavailable/truncated states. They contain neither raw
invocations nor endpoint configuration, credentials, interview answers or native
failure text. No report capture, preference write, optimizer decision or provider
request occurs as a side effect of inspection.

## Qualification and remaining scope

Compare projections with canonical policy and routing services on both stores.
Exercise multiple pages, altered actor/scope, changed policy or host ceilings,
missing or purged source evidence, child-binding unavailability and hostile text.
Check that the complete canonical state remains unchanged through the actual SDK
and compiled host, and that absent profiles fail before handler dispatch.

Optimizer report capture, revision-bound preview/apply/rollback and durable public
command reconciliation remain separate prerequisites. The full inspector UI,
retention invalidation, webview reload and encrypted cloud publication remain
governed by [P4-04](../plan/18-deferred-vscode.md#p4-04--inspectors).
See [the inspector contract](../development/editor-inspectors.md).
