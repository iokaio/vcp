# P5-07 retention and local cleanup

Retention previews materialize a normalized selector into exact canonical IDs,
dependent copies, protection reasons, source commitment, authority/deletion
revisions, estimated bytes and known backup obligations. Applying a saved preview
checks that same selection. New matching history cannot silently enter it. Saving
preview metadata is exempt from its source commitment; ordinary source changes
require a new preview.

Selectors distinguish stable workspace/root IDs from relative paths. Date-only
values require an explicit fixed offset and mean midnight at that offset;
timestamps require Z or a numeric offset. UTC bounds retain explicit inclusivity.
Missing metadata remains unknown under negation. Named timezone ambiguity is
rejected. [ADR-022](../adr/022-retention-selection-and-cleanup.md) records these
choices and their limits. Source selectors follow retained native capture metadata
by exact scope, hash and byte count even after the workspace binding changes.
Missing metadata stays unknown; multiple declared claim roots remain selectable.

Recall exclusion, presentation compaction and purge have separate effects.
Exclusion can be reversed; compaction retains the raw evidence and permits an
expanded view. Neither deletes payloads. Purge commits tombstones, a new deletion
epoch, context/generation invalidation and a durable exact cleanup job before
removing payloads. Current retrieval and prepared dispatch cannot reuse an older
epoch. Data already sent to a provider cannot be recalled.

The dependency scan follows event artifacts, claim provenance, typed memory
passages, included context artifacts and exact provider-request commitments.
It does not find derivatives by comparing arbitrary strings. Active recovery,
turns, unfinished capture and unresolved effect/accounting facts are protected.
A protected dependency refuses the whole atomic preview batch, even when it also
contains disjoint tasks. Directly protected and batch-blocked IDs are identified
separately; narrow the selector or reconcile the protected work before creating
a new preview. Pausing alone does not settle a liability.

## Physical removal

The [sealed replay-base decision](../adr/021-retention-replay-bases.md) preserves
canonical IDs, ordering and opaque original transaction receipt commitments
while replacing selected payloads with explicit typed redactions. Historical
transaction bodies are not reconstructed or falsely rehashed. Redacted events,
inspection results, tasks, turns, effects, verification details and immutable
memory records keep required provenance/outcome facts without retaining removed
text. Ordinary writes cannot mint these redactions. Settled accounting narratives
are erased separately from numeric adjustments and admission commitments. An
original observation digest preserves exact retry. A later new bill admits fresh
immutable evidence and can update accounting facts while retaining the historical
erasure marker; it cannot replace the erased narrative. Newly admitted narrative
can itself be purged after the resulting liability is reconciled.

Both backend preferences rewrite into a fresh private root, validate and reopen
it, then activate it. Older roots remain owned cleanup obligations. Snapshot and
artifact leases block removal while a reader still needs the old bytes; releasing
the canonical owner does not release another reader's lease. Cleanup resumes
from the committed exact job after restart and enumerates recognized files only
under authenticated retired roots or owned aborted staging children. Unknown
entries and locked files remain pending. It does not recursively delete arbitrary
directories or promise erasure of SSD blocks.

Generation retirement waits for canonical snapshots and native reader pins.
Receipts distinguish logical unavailability, rewrite progress, local payload
cleanup, pending generations and retained backup identities. Local cleanup does
not establish deletion of cloud provider version history. The portable snapshot
service must recheck the current deletion epoch before admitting a new copy.

## Notices and saved policy

Notification-only is the default. Retained history becomes due strictly beyond
thirty 24-hour days, including history excluded from recall. Acknowledgement is
persisted separately from content age; the explicit default repeat cadence is
seven 24-hour days and is configurable. Read-only inspection does not delete,
extract memories or start a provider request.

Automatic actions require a saved selector, action, cadence, granting actor and
authority revision. Queued applications recheck that exact policy before applying
their exact preview. Disabling or changing the policy prevents new applications;
already committed tombstones remain facts. The same planner and cleanup engine
serve explicit and configured applications.

The automatic driver runs when the canonical owner starts, after recovery and
before execution. Policy changes take effect at the next owner start; there is no
background service while it is closed. Blocked attempts persist their status and
retry cadence. Startup also retries at most four existing purge cleanup jobs;
pinned and remaining jobs stay pending for later or explicit cleanup.

## Qualification

The targeted governed-memory and policy tests passed on Files and SQLite,
including stale previews, active protection, immediate logical denial, independent
exclusion/compaction, the thirty-day boundary, durable repeat notices and policy
disable. The physical test retains a synthetic marker in an old snapshot, checks
cleanup remains pending, releases the pin, reopens the owner and independently
scans the remaining owned files for marker absence. Repeated cleanup and reopening
preserve the redacted state.

Eight real process exits cover both backends after the tombstone commit, before
activation, after activation and after a cleanup file removal. Each restart resumes
the original exact job and independently checks marker absence. The replay-base
regressions also verify a durable snapshot job survives owner restart and blocks
cleanup until its source references are explicitly released. Post-purge empty
generation rebuild acknowledges retained intent IDs without adding memory.

The integrated domain, protocol, store, accounting and memory all-target regression
run passed 149 tests with no failures. Two existing real-model qualifications were
explicitly ignored because that invocation did not provision model assets. The
run includes nonempty native lexical generation cleanup at its recorded owned
root and explicit pending status for external generation copies, late accounting
observations and subsequent narrative purge, historical source selectors, durable
policy execution and both-backend replay/pin recovery.

The complete canonical host regression passed 34 tests with no failures in
715.00 seconds; four existing asset-dependent cases were explicitly ignored.
The separate native publication gate provisioned the pinned MiniLM assets and
passed all sixteen actual process-kill boundaries across Files and SQLite in
39.07 seconds. The actual retained HTTP dispatch race rejects stale memory
context after retention without admitting a provider attempt. Startup-policy
and paused-owner controls passed on both backends.

Final audit, CLI and domain all-target tests passed 62 tests with no failures or
ignored tests. The CLI build, affected-package Clippy, changed-source formatting,
diff checks and eight deterministic delivery cases passed. Existing upstream and
pre-existing Clippy warnings remain visible; none were suppressed for this work.
These are native process-crash results, not power-loss or remote-copy erasure
claims. Portable encrypted snapshot/handoff integration is qualified separately
by P5-09/P5-10.
