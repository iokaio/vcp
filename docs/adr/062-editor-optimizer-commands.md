# ADR-062: Reconciled editor optimizer commands

Date: 2026-09-23
Status: accepted P4-04 API prerequisite; full inspector acceptance remains open.

## Decision

An optional `routing/optimizer/1` profile exposes local report capture, governed
report reads, exact previews, policy publication and rollback. It does not start
provider work, solicit remote advice, edit preferences or write configuration.
Report capture persists evidence and is a command, not an observational read.

Workspace optimizer commands require the authenticated connection's current
controller ownership. The worker derives access from the requesting actor and
checks current authority, workspace revision and root binding. It never borrows
the host owner's identity or promotes an observer. Capture and reads may inspect
local evidence in an untrusted workspace; new preview, apply and rollback require
current trusted workspace state. Effective previews require actual host routing
ceilings, not client-supplied limits or defaults invented by an inspection host.

Session observers may read only reports whose complete captured coverage remains
authorized. Captures remain bound to the original actor and session; an observer
with that actor may read session coverage. Workspace coverage is unavailable to an observer, including an empty
report. Every page revalidates source tasks, retained events and the saved forecast
artifact and manifest. A report is never trimmed and relabeled as a scoped
aggregate. Bounded typed pages omit raw payloads, preferences and native errors;
continuations bind caller, scope, report identity, current authorization and query.

Preview stores the exact internal proposal in a bounded connection-local cache
and returns an opaque handle and sanitized comparison. A preview is not dispatch
authority. Publication recomputes against current policy, preferences, source
evidence, trust, binding and host ceilings. A changed dependency requires a fresh
preview. Rollback publishes a new descendant revision; it does not rewrite policy
history.

Capture and publication atomically commit the standard actor-bound command
receipt with their canonical changes, using the caller's actual session. A
custom optimizer projection or a separately appended receipt is insufficient.
After current authorization, matching command replay precedes stale revision and
preview-cache checks. A lost response is reconciled through the original command
identity; reload never automatically repeats an unknown mutation. Unpublished
spool bytes are not an accepted capture. Acceptance reports the captured report
or published policy revision; callers reread workspace revision before a new
command.

The public report identity is the capture command ID. An atomic capture binding
maps it to the unchanged internal report identity, preserving forecast artifact
and manifest provenance. Acceptance revision is zero for immutable captures and
the newly published policy revision for apply/rollback; workspace preconditions
are a separate counter.

This replay contract applies to public optimizer commands. Existing CLI commands
retain their private receipt behavior; deliberately reusing an ID across the CLI
and public interfaces is not a cross-interface deduplication guarantee.

Policy publication preserves existing provider preparation and send-time checks
against the current policy and registry. Existing CLI controls do not clear every
prepared cache. Public commands must preserve those admission fences rather than
claiming cache eviction proves safety or disturbing active provider work.

## Qualification and remaining scope

Both stores must prove atomic receipt recovery, exact duplicate replay, conflicting
command rejection, caller-session correlation and source-governed pagination.
Adversarial checks cover expired ownership, changed authority/trust/binding,
concurrent CLI policy or preference changes, lost preview caches, purged sources,
empty workspace reports and malformed cursors. Existing provider preparation and
send fences continue reading current canonical policy; this increment does not
claim a newly exercised provider send race.
The actual SDK must negotiate the profile and reconcile committed commands after
reconnection without provider dispatch or configuration writes.

This prerequisite does not accept the complete inspector work item. UI review,
reload behavior and encrypted cloud publication remain governed by
[P4-04](../plan/18-deferred-vscode.md#p4-04--inspectors).
