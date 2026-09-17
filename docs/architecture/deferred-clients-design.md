# Deferred clients and execution-host design

Status: proposed design for [P9 API/SDK](../plan/17-deferred-api-and-sdk.md),
[P4 editor](../plan/18-deferred-vscode.md) and
[P10-04 hosts](../plan/19-deferred-extensions-and-platforms.md#p10-04--other-execution-environments).
All remain deferred until their ledger prerequisites pass. This document creates
no public protocol version, supported editor/OS range or new release prerequisite.
It refines [architecture section 5](vcp-what.md#5-engine-api-and-protocol),
[section 18](vcp-what.md#18-vs-code-extension) and the existing single-controller,
close-to-pause, data-scope and prepared-edit requirements.

## Ownership and adapter boundaries

The Rust controller remains the sole owner of scheduling, canonical state, memory,
authorization and accounting. Public methods validate and translate into the same
internal commands the CLI uses. An SDK or extension never executes a model request,
directly edits engine databases or allocates an independent task budget.

Distinguish connection identity, authenticated principal, controller lease,
workspace identity and execution host. Possessing a task ID or event cursor proves
none of those authorities. An observer can inspect only its authorized scope and
cannot answer a pending decision. A controller lease identifies interactive command
authority; it is not a reusable grant for arbitrary tool effects.

[Section 4.5](vcp-what.md#45-terminal-close-pause-and-workspace-resume) fixes owner
loss as a pause boundary, and
[section 5.4](vcp-what.md#54-events-reconnect-and-ownership) applies it to local
attachment. Preserve that close-to-pause default when moving the engine out of
process. A separate daemon process is not permission to keep tasks running; any
later explicit daemon-owned mode needs a documented product decision, visible
owner and equivalent stop rules.

Explicit pause keeps the controlling application and its connection open. The task
tree stops admitting new work while status, inspectors and event subscriptions
remain available. Resume is a deliberate engine command that reconciles effects
and revalidates current state; neither view reconnect nor a status query resumes it.

## Schema and request identity

Use one canonical schema source for envelope parameters/results, event payloads,
errors and generated TypeScript/JSON Schema. Keep protocol framing code separate
from domain types and generated files separate from SDK logic. P9-01 must select
and document the actual schema/generator tooling against the retained C01 machinery.
This document does not prescribe a dependency or a generated-file command that
does not yet exist.

Proposed common command fields are `command_id`, workspace/session/task scope as
applicable, `expected_revision`, and method-specific payload. Requests use standard
JSON-RPC 2.0 with a connection-scoped request ID; durable mutations additionally use
the command identity. Store a canonical payload digest, authorization scope, durable
result or operation ref and completion state for the command. A duplicate with the
same digest returns the existing acceptance/result under current read authority;
a different digest yields a conflict. Replay protection cannot depend solely on
an in-memory connection map or JSON property ordering.

Represent opaque IDs and 64-bit counters as strings and money through the ledger's
exact decimal representation. Decide absent/null semantics explicitly per field.
Classify evolution: optional presentation fields may be ignored; unknown required
capabilities, security states and authority-affecting enum values reject use. A
client must not map an unfamiliar effect state to `succeeded` or a missing permission
field to allow. Capability negotiation complements version agreement.

Build the method registry directly from
[the minimum method set](vcp-what.md#53-minimum-methods). For each method record
read/mutation classification, controller/observer requirement, workspace binding,
revision preconditions, idempotency rule, maximum input/result size and underlying
engine command/query. Query results still enforce current scope and retention.
Mutation acceptance means the command is durably recorded; long work reports its
terminal result through events/query rather than holding a connection indefinitely.

## Framing and compatibility

Use bounded UTF-8 JSON-line framing for stdio and the qualified Windows pipe
transport. Retain partial reads until a complete frame or a size violation; limit
buffer growth before JSON decoding and reject invalid UTF-8/JSON deterministically.
Escaped string newlines do not delimit messages. Separate envelope validation,
initialization/version checks, authentication/scope checks and method decoding so
malformed input cannot reach execution.

Diagnostics go to stderr or a local diagnostic sink, never protocol stdout. Large
artifacts use bounded access-checked reads; negotiated envelope size is a hard
ceiling, not an invitation to embed a full transcript. Define handling for unknown
methods, notifications, batch envelopes and out-of-order responses explicitly in
the compatibility profile and test against the JSON-RPC standard before publishing.
Do not advertise a general codec feature that the dispatcher silently ignores.

Initialization returns negotiated protocol/event/schema versions, engine build,
available capabilities, frame/subscriber limits, execution host and actual sandbox
capabilities. No workspace action runs before initialization. Maintain a supported
version matrix with old/new peer fixture pairs; distinguish unsupported major
versions from optional features absent on an otherwise compatible peer.

## Local authentication and controller leases

P9-02 must choose and qualify local endpoint security on native Windows, including
pipe namespace, endpoint permissions, peer identity, inherited handles and launch
bootstrap. A pathname, PID or random pipe name alone is not authentication. Use
selected, reviewed platform primitives; pass any bootstrap credential through a
restricted channel rather than argv, environment dumps, logs or workspace files.
Stdio ownership also requires a controlled launch/handle relationship.

A proposed lease record binds session, authenticated controller, lease generation,
expiry/owner-liveness policy and current revision. Acquisition/transfer/expiry are
canonical operations and events. Resolving a question or approval checks the live
generation plus pending decision, operation and policy revision atomically. A
second controller receives an explicit conflict or a deliberate transfer flow;
it does not win by last writer. Losing an observing connection changes no task
authority. Losing the owner blocks new work and invokes normal pause/recovery.

One writer lock per canonical data root is acquired before engine mutation. Attach
connects to that engine or returns a conflict; it never opens a second independent
writer merely because the endpoint is inaccessible. Lock and endpoint recovery
verify ownership/liveness and retain recovery evidence before replacing stale
metadata. Local attachment adds no TCP, hosted or remote authentication support.

## Events reconnect and cancellation

Each subscription binds authorized session scope, event schema and an after-sequence
cursor. Initial snapshot and replay need a consistent boundary: read a snapshot at
canonical sequence S, then deliver events after S without missing commits between
those operations. Deduplicate allowed replay by event ID and scoped sequence; do
not assume one global counter across sessions or memory.

If requested events were pruned, return an explicit gap plus a fresh snapshot
boundary and retained artifact availability. Bound per-subscriber queues and
outstanding requests. A slow reader receives an explicit resync/disconnect outcome;
it cannot block canonical commits or force unbounded buffering. Token deltas may
coalesce, while completed observed output remains an inspectable artifact subject
to current retention. Access revocation stops further delivery and cached reads.

A transport request timeout is not a task timeout and does not establish whether a
mutation committed. Query the durable command or returned operation before retrying
with the same identity. Distinguish abandoning a local SDK await/subscription from
issuing an engine turn/task cancellation. Document this distinction in types and
examples. An explicit cancel uses its own durable command identity and awaits the
engine's cancellation/reconciliation result; unknown effects remain unknown.

The SDK owns connection disposal, bounded decoding, pending request correlation,
cursor persistence hooks and structured errors. It exposes user-controlled retry
policy and operation handles; no hidden new idempotency key on reconnect. Multiple
simultaneous consumers do not each become the controller. SDK shutdown releases
resources and follows the negotiated ownership mode rather than implicitly killing
an unrelated engine or approving unanswered questions.

## Workspace and document projections

Map editor folder URI and execution host to a durable engine workspace/root binding.
Folder display names are presentation only. Nested or identically named roots
require unambiguous resolution; symlinks and moved roots follow engine identity and
rebind rules. Multi-root grants and memory remain separate unless explicitly scoped
together. Initially qualify only the local Windows host. Other URI schemes and
remote hosts require P10-04 rather than guessed path translation.

Workspace trust is an input to effective capability policy, not a substitute for it.
Trust revocation stops new executable project behavior and revalidates prepared
operations. The extension can display authorized state in restricted mode without
starting project-defined commands. Resolve engine location/configuration from
trusted sources; a repository file cannot choose an arbitrary executable simply by
claiming to be VCP configuration.

Proposed `DocumentObservation` fields are workspace/host, URI, language, document
version, content hash, dirty flag, selection ranges, optional bounded content or
ephemeral ref, disk fingerprint and observation revision. Diagnostic observations
also identify document revision, producer and observation time. Save, rename,
close, delete and workspace-rebind events invalidate relevant projections.
Collection must preserve content encoding/line-ending evidence where it affects
application or disk checks; an editor's logical text is not automatically identical
to file bytes.

The engine records whether context used a disk artifact or an unsaved document.
Unsaved drafts are ephemeral unless the user requests capture, consistent with
[section 18.3](vcp-what.md#183-buffer-context). Record provenance and permitted hashes
without silently retaining the complete draft as a permanent memory fact. Any
requested full capture remains subject to normal scope, retention and user-visible
capture policy. Reopening a URI does not prove its old document version is current.

## Prepared editor edits and receipts

An editor change proposal carries operation/change-set ID, workspace/host, expected
document versions/hashes, expected disk fingerprints, edit contents or artifacts,
authorization/policy revision and requested apply mode. Prepare and preview are
separate from apply. Immediately before apply, the extension validates mapping,
trust, live documents and expected revisions; the engine revalidates operation
authority. A changed document causes conflict/reprepare, preserving user typing.

P4-03 must qualify the chosen editor API's actual concurrency guarantees. Do not
promise compare-and-swap or cross-file atomicity from a high-level edit API name.
If a safe version-bound operation cannot be provided, require a refreshed review
or explicit save/retry boundary and disclose that limitation; never emulate safety
by overwriting newer text after a failed check.

Proposed receipts contain operation identity and per-resource before/after document
versions, content hashes, disk state if saved, observed disposition, errors and
unknown outcomes. The engine persists dispatch intent before asking for an editor
effect and persists the receipt before marking it applied. Duplicate requests query
the operation receipt and current document state; they do not apply an insertion
twice. Crash after editor apply but before receipt yields reconciliation work.

Multi-file partial application remains partial, with successful resources and
conflicts recorded independently. Any rollback is a newly prepared edit against
current versions, because typing or undo may have occurred meanwhile. Unsaved
buffer edits cannot satisfy a disk-write receipt. Save/undo/redo invalidate affected
verification; running tests against disk does not verify divergent dirty buffers.
Completion states exactly which representation and fingerprint were checked.

## Presentation and package compatibility

Prefer native editor document/diff/diagnostic APIs where they fit. The webview only
receives bounded presentation records, opaque action IDs and authorized artifact
ranges. Validate every inbound message against a closed schema, attach current
decision revisions in the extension host and reject invented action IDs. Treat
model/tool Markdown as untrusted; constrain scripts, resource access and links,
and never allow output HTML to generate an executable command implicitly.

Recreate views from engine snapshots/events after reload. Keep questions and child
progress attributed; disable a submitted control while retaining the command ID
for safe reconciliation. Current engine state determines completion. Inspectors
use engine services for cost, policy, routing, history and memory, with access
rechecked on every page/read. Revocation/pruning removes visible cached text and
prevents later replay from extension storage. Provider/recovery credentials never
enter webview state or logs.

P4-05 chooses engine discovery/distribution and the supported extension/SDK/protocol
matrix with install evidence. Package independently of a development checkout,
declare engine requirements and verify executable provenance before any managed
installation/update. Preserve configured data roots and recovery-key separation;
an extension uninstall or update cannot delete them. Failed upgrades retain a
usable prior installation or clear recoverable state, subject to store migration
compatibility rather than an assumed binary rollback.

## Execution-host contract

P10-04 extends host adapters without changing workspace/task/memory IDs. A proposed
`HostCapabilities` record names host/OS/version, launch modes, path/case/link rules,
process-tree controls, filesystem/network enforcement, PTY support, credential
provisioning, local inference/index support and transport limits. Capabilities are
evidence-backed facts; unknown enforcement is unavailable for a required policy.

Keep UI host, engine/store owner and execution host explicit. Map resource identities
as `(host_id, workspace_id, root_id, relative_path)` until the owning host resolves
them. Never apply a local Windows canonicalization result to a remote POSIX path.
Test case collisions, Unicode, links, drive/UNC boundaries, mounts and recreated
containers using the actual host's rules. Reject ambiguous mapping before dispatch.

Remote/environment effects use the same durable intent/receipt lifecycle. A network
disconnect cannot prove a command stopped; reconcile through an authenticated host
operation identity and independent observations, or preserve unknown outcome.
Liveness detection must stop new root/child work after owner loss. A remote worker
that cannot honor the qualified owner-loss boundary cannot be advertised as fully
supported by merely disconnecting the UI.

Environment transfer rebinds roots, host identity and local authority deliberately.
Existing history, child state and liabilities survive. Secrets are provisioned by
scoped reference on the destination; do not copy an ambient UI environment. Local
working data stays plaintext; cloud-bound transfer uses the existing encrypted
snapshot publisher and externally held recovery material. Native index versions,
model/tokenizer identity and host compatibility determine reuse versus local rebuild
from retained canonical inputs. An incompatible index is not an empty memory store.

Each advertised host needs real install/upgrade, path/permission, process-tree,
disconnect/recovery, toolchain and inference/index evidence. Test WSL, SSH and
devcontainers as separate placements, not aliases for native Windows. Record
unsupported controls and not-run environments; retain Windows CLI regression gates
when shared abstractions change.

## Qualification and open decisions

P9 must resolve schema generation, version/evolution profile, local authentication,
lease/liveness limits and supported JSON-RPC behavior. P4 must qualify editor apply
semantics, ephemeral buffer capture, package/distribution strategy and version range.
P10 must qualify each host's enforcement and transfer support. Record those choices
in their owning ADRs with concrete fixtures; none is implied by this proposal.

Contract fixtures cover malformed/oversized streams, same/different-payload retries,
unknown governance states, denied observers, lease replacement, gaps and backpressure.
Use actual engine/SDK processes for transport and ownership proofs. Editor tests
exercise real typing, dirty save, undo, reload and partial apply; mocked webview
messages cannot prove preservation of user buffers. Native host tests observe actual
processes and files independently of returned receipts. All synthetic traces preserve
the same current-result, cost, scope and close-to-pause invariants as the CLI.
