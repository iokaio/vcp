# ADR-063 — Scoped encrypted publisher commands

Date: 2026-09-23

Status: accepted P4-04 API prerequisite; full inspector acceptance remains open.

## Context

P4-04 routes cloud-directed exports through the existing encrypted publisher.
The native publisher already captures a governed workspace checkpoint, encrypts
an archive, records native copy ownership and reconciles an independent trust
checkpoint. Its background operation previously belonged only to the native
host. Public command acceptance, authenticated caller ownership and editor lease
loss require an additional boundary before exposing this pipeline to clients.

## Decision

Negotiate `backup/publisher/1` for `backup/status`, `backup/create`, `backup/read`,
`backup/retry` and `backup/cancel`. These commands publish a **whole workspace**
backup to the configured local encrypted vault. They do not export only the
selected session. `session/export` remains a separate unencrypted derived export.

The executable loads one explicitly selected native publisher profile containing
local recovery-key and Git references. The bounded profile and referenced files
must satisfy the native placement and no-redirection checks. Enrollment, vault,
staging and configuration revisions come from the existing independent trust
store. Public RPC accepts no key values, key paths, vault paths or replacement
capabilities. A fresh opaque reference identifies each load. The editor-facing
result contains only that reference, generation and configuration revision.
Observer attachment never loads recovery material or acquires control.

`backup/create` atomically records a caller/session-bound durable intent and a
standard command receipt **before** scheduling checkpoint capture. The operation
ID is the original command UUID. Retry and cancel use new command UUIDs and
explicit target intent/job revisions. The common mutation revision is the
workspace revision; acceptance reports the intent revision. Exact receipt replay
precedes stale revisions and volatile capability checks and never starts work.
An accepted intent interrupted before scheduling remains queryable; acceptance
alone does not prove that a native snapshot job exists.

Every public background operation retains an additional authority guard. Source
capture, stage acceptance, publication admission and copy-identity recording
recheck the initiating public connection/controller lease, caller scope, trust,
authority, deletion epoch, binding and loaded capability. Archive, encryption and copy
loops poll cancellation. An already running bounded repository observation may
finish, but its results are rejected after authority loss; reobservation requires
a fresh guard check. A rejected guard stays rejected. Existing native-owner
backup calls preserve their existing behavior.

Once copy admission happened, revocation cannot retract bytes already exported.
Completion, independent checkpoint reconciliation and proven source release may
record what happened after authority loss; they cannot grant new publication.
Cancellation records stop intent and signals the matching running worker on the
serialized owner. Its receipt does not assert deletion, final cancellation or
cleanup. Interrupted inactive jobs can require explicit retry/native maintenance.
No public command deletes a vault copy.

Only the authenticated operation actor in its original workspace/session can
read its projected status. A same-actor observer may reconcile existing IDs
without taking control. Status hides unrelated native operation IDs. Responses
contain fixed bounded metadata, never source inventories, native paths, raw
errors or secret values. Reads do not trigger recovery or filesystem maintenance.

Publication, cancellation, source pins and cleanup are separate facts. A
published job with retained obligations is not complete. Checkpoint match on a
released published job describes the reconciliation at release, not the latest
independent trust head after later backups. Authority/deletion fields describe
the native snapshot cut when present, otherwise the accepted intent. No native job means
source/checkpoint evidence is not observed, rather than proof that capture never
started. Historical local publication is evidence of a local encrypted object;
`cloud_transfer` remains `unknown` and restore verification `not_observed`.

## Consequences and qualification

The CLI and SDK share the existing engine publisher; no provider, cloud connector
or second encryption implementation is introduced. Clients reconcile
`command/read` and `backup/read` after lost replies or reload. They never retry,
reload keys or acquire controller ownership automatically.

Qualification covers both canonical stores, durable intent before scheduling,
exact replay and conflicts, real encrypted publication, scoped observer reads,
native profile rejection, and authority loss before source/copy admission.
This ADR does not accept the inspector UI or claim remote upload/restore tests.
Measured evidence belongs in the editor inspector development record.
