# ADR-046: Atomic metadata session forks

Date: 2026-09-23
Status: accepted for the session-fork increment of P9-02; the work item remains in progress.

## Decision

`session/fork` creates an ancestry-linked session and one pending root task from
an authorized, retained completed turn. It is a metadata operation. It does not
start execution, change the host's selected root, allocate a budget, or transfer
controller ownership, approvals, grants or unresolved provider liabilities.

The source session remains the command's scope. The caller supplies new session
and task identities, the completed turn, and a durable command identity. Creation
uses zero expected and steering revisions, as `session/create` does. Current
authority and controller ownership are checked before receipt replay; reuse with
a different payload conflicts. The original receipt is reconciled in the source
session, including after restart.

The engine proves the completed boundary against its retained event and exact
turn fact. It selects the task snapshot at that boundary, including the objective
for that turn's steering revision, fingerprint, editing mode and required checks.
A later objective or fingerprint cannot silently replace historical input.
Missing, suppressed or redacted required evidence prevents a new fork. The new
task starts at revision and steering zero with an explicit source-task link; its
objective is attributed to the new genesis event.
Normal owner shutdown or reopen recovery may subsequently pause that pending
task; the fork introduces no exception to those existing safety transitions.

One canonical transaction inserts the session and task, emits source acceptance
and target genesis events, and records the original command receipt. Existing
receipt isolation normally rejects transactions crossing session scope. A narrow
typed validation rule admits only this exact two-event, two-insert genesis shape
in the same workspace. It verifies the completed source boundary, ancestry,
record/fact equality, event causation and creation-only revisions. Malformed
reserved fork facts fail closed. It cannot update an existing target session or
carry unrelated authority or budget mutations.

The source event correlates to the public command. Target genesis uses a
versioned deterministic internal correlation and points to source acceptance by
causation. The receipt's sequence range therefore stays in the source session;
target readers cannot obtain a source-scoped command receipt through its genesis
event. Both stores commit all rows, events and the receipt together.

## Consequences and evidence boundary

A successful fork proves durable metadata and ancestry. It does not prove that
all historical context remains available for a later model request. Future
execution must explicitly select the new root, establish its own budget and
authority, reobserve the workspace and revalidate retained history. The existing
bounded history reconstruction and retention rules remain effective.

Native engine, store and compiled-process tests qualify this increment. Their
results are recorded in the [local attachment guide](../development/local-attachment.md).
This decision does not close the remaining P9-02 method adapters or acceptance
scenarios, and does not establish P9-03 SDK compatibility.
