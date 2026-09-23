# ADR-054: Scoped public retention

Date: 2026-09-23
Status: accepted for the retention increment of P9-02.

## Decision

The local API negotiates `memory/retention/1` for `memory/forgetPreview`,
`memory/forgetPreviewRead`, `memory/forgetRead` and `memory/forget`. These adapters
use the existing retention selector, exact preview, logical deletion, rewrite and
cleanup workflow. They introduce no independent deletion implementation.

A preview is read-only and belongs to one authenticated connection, actor and
workspace/session/task ceiling. The server caches at most two previews, each at
most 1 MiB with 8,192 selected/dependent identities, for 60 seconds. Pages expose
at most 128 identities from that frozen selection. Page reads revalidate current
authority, deletion state and target scope. The server never reruns a selector to
silently replace an expired or stale preview. Private generation and backup
identities remain internal; job reads expose counts and cleanup status.
Before the shared source commitment is canonicalized, public preview and fresh
apply check a borrowing serialization ceiling of 16 MiB and 32,768 total retained
record/event/command entries. Oversized histories fail with a resource limit;
accepted-command reconciliation remains available independently of this bound.

The shared workflow computes dependency closure before applying the authenticated
task ceiling. Any foreign dependency denies the whole operation; dropping that
dependency would leave an unsound partial deletion. A retained ceiling is evidence
of original scope, never a grant. Cleanup reauthorizes the selected targets and
only collects a search generation when its full source inventory is authorized.
Mixed or unprovable generations remain pending for separately authorized
workspace maintenance.

Applying a preview requires the current controller, original preview digest and
current revision guards. The original public command receipt commits in the same
transaction as the logical deletion, durable job and indexing intents. Retries
check current authorization and that receipt before consulting the ephemeral
preview cache. They return the original job and can continue its idempotent
cleanup without creating another deletion epoch. Changed payloads under the same
command identity conflict. A job read never drives cleanup.

The lifecycle owns an admitted mutation even if its RPC waiter is abandoned. When
MCP connections exist, it holds runtime work before applying the exact preview;
canonical task pauses follow logical acceptance so that the pause itself cannot
invalidate the preview's source digest. Cleanup runs after owned dependency
drain and fresh authorization. Lost authority leaves an accepted job pending,
with work held and an outcome requiring original-command reconciliation. It
never resumes execution automatically.

## Verification

Native qualification passes 32 protocol tests and five both-store shared tests
covering read-only previews, stale sources, foreign dependencies, receipt replay,
physical rewrite/reopen and scoped generation cleanup. Borrowing source-limit
tests and existing publication/retention regressions also pass.

Two compiled bridge tests pass on both stores (61.41 seconds), covering profile,
observer and lease denial, unique bounded pages, stale previews, cache eviction,
physical purge, job reads after task redaction, original-command replay after
reconnect and foreign-session copy denial without disclosure or writes.

One native lifecycle test passes four real MCP scenarios (both stores, abandoned
waiter or controller loss; 3.40 seconds). It blocks owned dependency drain, observes
durable acceptance and immediate hold, abandons the response, then releases the
dependency and reconciles with the original command. Cleanup completes without
automatic resume or another deletion epoch; subsequent replay preserves the
canonical watermark. Required current lease revisions remain enforced.
