# ADR-083 — Shared provider request pacing

Status: accepted October 3, 2026 for the owner's concurrent scenario requirement;
P2-02 provider admission and BETA-03C installed workflows.

Independent workspace owners must be able to run together. Their canonical
stores, authority, budgets and project files remain independent, but their
OpenRouter requests compete for the same user's provider capacity.

Windows CLI execution therefore installs a user-local coordinator beneath
`%LOCALAPPDATA%\VCP\account\provider-pacing`, independent of `--data-dir`.
Two OS-held file leases bound active model transports. Additional requests wait
asynchronously before canonical reservation or submission. Starts are spaced
by 250 ms. This conservative local bound is not a claim about upstream limits.
All selected models share this bound; no credential, account identifier or
project content is persisted in coordinator state.

Each HTTP 429 publishes a shared cooldown of at least five seconds. Supported
longer Retry-After values are preserved. Unsupported hints still forbid that
attempt's automatic retry; retained error-response evidence remains available. Existing
per-attempt retry counts, absolute deadlines and owner-generation checks remain
authoritative. A queued stop or deadline creates no attempt or billing liability.
Initial calls, compaction and retries use the asynchronous admission boundary;
resume grants forward it. Tool and build execution do not consume provider slots.

The coordinator uses pinned local paths, bounded versioned state, serialized
updates and OS file locks. Reparse points, hard links and corrupt state fail
closed. Process death releases transport leases. Cooldown updates have a bounded
lock wait; failure denies another send while retaining response evidence and
the existing canonical accounting transitions.

Pacing cannot guarantee availability in a provider's shared upstream pool.
An already submitted error without trustworthy final cost remains an unresolved
canonical liability. This decision neither treats 429 as free nor admits extra
spend. Recovery/reconciliation of those earlier requests is a separate operation.

Qualification uses independent owners, real child-process death, shared cooldown,
queued deadline/cancellation with zero canonical reservations on both stores,
and existing retained-liability regressions. No live paid concurrency claim is
made from scripted transport tests.
