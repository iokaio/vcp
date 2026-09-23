# Local attachment construction

Owning item: P9-02. The protocol foundation is delivered in PR #134. This increment
adds canonical controller-lease state, a live connection adapter, controlled
Windows launch and named-pipe attachment to the same live writer. P9-02 remains
in progress: attachment, owned resume and bounded subscriptions have native
incremental qualification below. Remaining method adapters
and SDK acceptance are separate work.

## Controller identity

A lease is a typed canonical `Access` document for one workspace/session. Its key
is a deterministic digest of that scope and document type, separate from policy
and grant keys. The document stores its revision, monotonically increasing
acquisition generation, and optional holder. The holder binds authenticated actor,
connection identity, current engine process owner and owner epoch. Connections
receive their identity from the trusted transport host, never from method params.

Acquisition conflicts with an occupied lease. Deliberate transfer consists of
release followed by a new acquisition; acquisition does not resume work. Release
keeps the last generation, and the next acquisition increments it. A stale process
holder requires explicit recovery before a replacement can acquire the lease.
There is no timeout that silently promotes an observer to controller.

Lease operations require current write access before receipt lookup. A repeated
operation returns its original durable receipt without changing the lease. That
receipt is evidence of an operation, not a present authorization grant. Obtaining
and checking an opaque controller token separately checks the current holder,
generation, connection, actor, scope and engine owner. Old tokens fail after loss,
transfer or engine restart. Observers cannot obtain a token by replaying a
controller command ID.

The canonical store validates lease shape, scope references and successive
revisions/generations, including replay from disk. A generic `Access` document
cannot replace a typed lease at its reserved key. Lease events and their command
receipts commit together. Only `controller-` followed by 64 lowercase hex digits
is reserved in the `Access` collection. An existing conflicting generic record
fails closed; the store does not reinterpret or rewrite that history. Other
generic keys retain their existing behavior.

## Live host integration

RPC framing, initialization and method negotiation remain separate from host
execution. `RpcHost` supplies current authorization and typed call dispatch. The
existing direct-engine adapter remains useful for protocol tests; the production
adapter lives in `vcp-lifecycle`, which already depends on `vcp-engine`.

The live adapter must validate controller generation in the same serialized worker
operation as decision/task revision checks. Bare engine mutation is insufficient
for steering, pause, cancel and resume: existing lifecycle methods fence retained
execution, own draining, and revalidate continuation. Public command identity must
survive those operations, including the pause-adjusted revision during steering.
An interrupted authority-change intent remains reconciliation work.

Connection loss must hold and pause owned work while retaining the host for
observers and recovery. `CanonicalOwner::close` is host teardown, so it cannot be
used for a reconnectable connection loss. Canonical non-running task state is a
necessary lease transition precondition; it does not prove that retained effects
have drained. The host must establish that independently. A token never replaces
runtime execution admission, budget checks or effect reconciliation.

`PublicConnection` selects public ownership only on a quiescent private host.
The retained `CanonicalOwner` continues to own the writer; authenticated observers
can read without a controller lease. Mutations require both the durable lease and
the connected host owner, checked inside the serialized worker. Refreshed read
access cannot upgrade the connection's original scope or role, or refresh an old
controller token. Public mode also gates startup and effect admission.

Live steering validates the original request before holding work. It records an
authority-change intent containing the original command identity and digest,
pauses canonical tasks, drains retained work and scheduler completion, then
revalidates authority before committing. An opaque engine-local pause proof binds
the adjusted task revision to that original request. It cannot be supplied over
the wire. Dropping the caller's await leaves completion owned by the host; losing
the controller or fencing the worker prevents the final steering commit. Receipt
replay does not perform another hold. An intent alone is not a successful receipt.

Connection loss invalidates admission immediately and starts interruption before
waiting on the writer queue. It releases the durable lease only after owned
draining; observers retain access to paused state. Reacquisition never resumes
execution. If disconnection interrupts construction before the first retained root
attaches, generic startup remains held. The owned-execution path below can mint a
new constructor-specific grant only through explicit resume after draining; old
constructors cannot use that grant or attach through the generic path.

## Atomic metadata forks

`session/fork` requires the source session's current controller and an authorized,
retained completed `through_turn`. Supply `new_session`, `new_task` and the original
mutation command ID, with expected and steering revisions both `"0"`. Acceptance
creates an ancestry-linked session and a pending root task atomically. The new
objective, editing mode, required checks and fingerprint come from the task
snapshot at the completed boundary, not from later steering or workspace changes.
Missing or suppressed boundary evidence returns an explicit unavailable result.

The operation leaves the host bound to its source session. It does not start the
fork or copy its source's budget, controller lease, approvals or provider liability.
Execution requires separate explicit root selection and fresh admission. The
metadata receipt also does not guarantee that a later historical context read
will succeed after retention changes.
Pending is the creation state; ordinary shutdown/recovery may subsequently pause
the task, using the existing lifecycle rules.

Reconcile the original command in the source session. Repeating the same payload
returns its original receipt; different parameters with that ID conflict. Current
authorization is rechecked before replay. A collision with an existing target
cannot leave an orphan session or task. Target genesis has its own deterministic
internal correlation and points back to source acceptance by causation, preserving
source-scoped receipt visibility. The exact transaction contract is recorded in
[ADR-046](../adr/046-atomic-metadata-session-forks.md).

Native Windows/Rust 1.95.0 verification passed 94 engine/protocol tests, 76 store
tests and a compiled-process test covering both stores. The fork tests prove
historical selection after later steering, atomic collision rejection, concurrent
duplicate admission, retained-boundary failures, restart replay, current-controller
requirements and scoped receipts. Store tests reject 13 forged transaction shapes.
Two existing environment-dependent store tests (real OneDrive and independently
provisioned age/Node envelope verification) remained ignored; they are not fork
qualification evidence. The 18-case fast suite, affected Rust formatting and diff
checks pass. Public DTOs and generated protocol artifacts are unchanged.

## Prerequisite verification

On native Windows with Rust 1.95.0, the domain, protocol, engine and store package
suites passed 150 tests, including five controller scenarios on both backends,
seven store lease contracts and two RPC host-interface tests. Two existing
environment-specific store tests remained ignored: real OneDrive and independently
provisioned Go age/Node envelope qualification. Neither is lease acceptance.
The 18-case repository fast suite and affected Rust formatting checks passed.

The lease tests establish writer exclusion, competing connections, scoped access
before receipt replay, generation changes, stale tokens after authority changes
and restart, explicit recovery, and rejection of Running-task ownership changes.
Store tests reject unreachable snapshot states, reserved-key occupation, forged
replay and type replacement without changing canonical state. These are native
library tests; they do not establish local peer authentication or live owner-loss
handling.

The live-host increment passed 68 engine/protocol tests and 30 lifecycle tests on
native Windows with Rust 1.95.0. The latter include seven public-connection/RPC
tests, one existing public metadata test, and 22 existing controller, integration
and port tests. Public steering fixtures cover both stores with an active
synthetic provider stream, a deliberately retained admission permit, dropped RPC
receivers, connection loss and a real output-capture capacity failure. They check
single receipt acceptance, unchanged steering on failure, retained partial
response/unknown provider liability, and the original authority intent. A no-root
fixture proves startup drains before acceptance; a separate fixture rejects late
root attachment after disconnect. These tests require no paid provider or external
test machine. Process-level endpoint authentication remains separate acceptance.

## Connected task controls

The live host advertises `task/read`, `task/cancel`, `turn/pause` and `turn/cancel`.
Reads expose scoped canonical task state, observed pending approvals and effect
uncertainty. The current turn is selected from retained canonical turn-creation
history; missing chronology yields a null turn rather than an invented ordering.
Turn mutation requires the latest provable turn in that task and current steering.
A stale, foreign or unprovable turn is rejected before retained execution is held.

Turn pause pauses the selected task and holds its retained descendants. Turn cancel
and task cancel cancel that task and its canonical descendants. Both use the same
retained fence as CLI stop. Their receipt acknowledges durable intent, while owned
interruption and effect reconciliation may still be draining. The authenticated
connection and its controller lease remain live; reads and receipt reconciliation
continue. Repeating a durable command returns its receipt without another hold.
Acceptance and terminal task state do not settle unknown provider/tool liabilities.
Resume is a separate explicit, revalidated operation, advertised only when the
server has the owned execution configuration described below.

Native Windows/Rust 1.95.0 qualification passed 76 engine/protocol tests, 13
public-host selections, and seven compiled process tests (five stdio attachment,
two task-control cases exercising both stores). Active synthetic root/child streams
prove hold, drain, replay and preserved uncertainty; no-root fixtures prove startup
is sealed and late attachment rejected. Compiled offline fixtures prove scoped
inspection, cancellation/replay, observer denial and a still-readable connection.
They do not claim compiled live execution/resume qualification. All 18 fast delivery
checks, affected Rust formatting and the existing both-store CLI stop/resume
regression pass.

## Durable canonical resume prerequisite

`PublicConnection::resume_canonical` accepts a typed resume intent at the trusted
native host boundary. It preserves the caller's durable command identity and
checks current controller authority before replay. Replay precedes retained binding
lookup, effect reconciliation, hold release and MCP proof collection. Host-local
serialization prevents concurrent duplicates from repeating those observations.
The same shared question-freshness predicates now serve CLI and host resume; old
or expired approvals do not become a new resume blocker.

A new acceptance requires a suspended task, current fingerprint and environment,
remaining budget, reconciled effects, actionable questions answered, and a retained
owner whose interruption and outstanding work permit continuation. The final
lifecycle guard stays held across canonical acceptance. An uncertain commit seals
admission for receipt reconciliation. The existing MCP exception still requires
proof of the exact owned idle process and granted undelivered call, with slot and
lifecycle guards retained through the decision.

This primitive never creates a retained thread, submits a model turn or dispatches
an approved tool call. The bare adapter does not advertise `session/resume`; the
owned-execution server adapter below adds submission. The primitive's native qualification
passes 80 engine/protocol tests and 19 public-host tests on Windows/Rust 1.95.0.
Both stores cover concurrent same-key resume, stale authority, unfinished retained
work, unknown effect liability and real MCP startup/call approvals. Independent
markers prove canonical acceptance and replay do not submit a tool effect.

The real MCP test exposed an idle daemon retaining its scheduler lease during
public owner cleanup. Release, disconnect and steering now close owned MCP
connections before waiting for scheduler quiescence, preserving host credentials
for later use. Both explicit release and disconnect drain the actual fixture
process. Existing CLI control and exact MCP approval regressions pass, as do all
18 fast delivery checks. This prerequisite does not close P9-02.

## Owned local execution construction

[ADR-045](../adr/045-owned-local-execution.md) selects the optional execution
bootstrap and server-owned submission boundary. A controller launch may include
`execution: {profile, provider_credential, credentials}` in its bounded private
bootstrap frame. `profile` is an absolute trusted profile path; `credentials` maps
configured MCP/decision names to credential material. Values remain memory-only
and never enter attachment grants, workspace files, argv, inherited environment or
logs. Missing named material fails closed; the server has no environment fallback.
Ordinary launch without this object does not prepare or start a provider.

The server pins profile bytes and checks the selected durable policy, workspace
binding and model configuration. It supports execution of its configured root,
not arbitrary task selection. Authentication and lease acquisition remain separate
from explicit `session/resume`. Resume preflight returns an authorized receipt
replay or an opaque original controller-bound ticket before session construction.
The first constructor uses a one-use startup adapter while generic startup stays
held; attachment rechecks its identity and authority and leaves the root held.
The canonical resume commit then revalidates continuation and distinguishes new
`Accepted` from `Replay`. Only new acceptance schedules a submission.

The supervisor owns that operation independently of its response waiter and is
the sole retained event consumer. Completion/abort handling is correlated with the
identified submitted turn, so stale queued events cannot affect a later resume.
Thread configuration is installed once after first acceptance. A durable receipt
means acceptance, not successful submission or completion: clients inspect the
current task/turn and command result. Submission failure pauses; replay and restart
never automatically repeat execution. A subsequent explicit resume does not renew
the server's configured execution window.

The trusted coding host also accepts an explicit canonical turn identity, rejecting
an existing identity in any scope before capturing input. Retained event correlation
IDs remain separate. This shared primitive prepares new-run admission; it does not
advertise `turn/start` or supply durable public acceptance by itself.

Each new resume admission has a 30-second watchdog covering profile preparation,
construction and submission. Receipt replay bypasses the watchdog.
Expiry invalidates the controlling connection and seals admission while the owned
future retains cleanup.
After five further seconds without completion, the dedicated server exits. Shutdown
uses the same bounded process fallback. This avoids dropping an unverified
constructor future; forced termination requires canonical crash recovery and is
not evidence that effects completed or liabilities cleared.

Shutdown holds admission and drains the event consumer, then closes the canonical
owner while its retained session remains alive. Only afterward does it shut down
the retained thread. The bridge reports a nonzero server exit without forwarding
private child diagnostics to the public transport.

A deliberately interrupted provider response is readable after restart only with
an exact canonical acknowledgement linking its physical descriptor to the attempt,
reservation and retained ledger liability. Reads and command replay do not clear
that liability. An unresolved acknowledged response still blocks new execution;
failed, mismatched or unlinked captures retain the hard recovery fence.

Native Windows/Rust 1.95.0 verification passes 89 engine/protocol tests, 25
public-host selections, 91 CLI unit tests and 14 compiled local-process tests.
The process tests cover stdio, named pipes, connected controls, snapshot replay,
artifact/usage reads and two synthetic execution cases. The execution cases prove
an actual file edit occurs once, connected pause remains readable, owner loss
blocks work, and fresh-server receipt replay does not resend. A fresh resume after
restart remains denied while provider liability is unresolved.

Additional passing checks cover both-store real budget-service accounting and
restart, ten audit-history tests, two capture-recovery proof tests, the seeded
provider and child recovery matrices, two existing partial-capture regressions,
and the shared noisy/quiet child event-owner regression. CLI unit output includes
two ignored entries: an inherited-handle subprocess fixture invoked by its parent
test, and the existing external OneDrive-root qualification, which was not run.
All 18 fast delivery checks pass (manifest
`artifacts/tests/9b7b7144-cbec-4b8c-8c76-f826e711c59e/manifest.json`), as do Rust
formatting, compiled schema regeneration/drift checking and TypeScript 5.9.3 strict
checking. No paid provider qualification is implied. These increments do not close
the remaining P9-02 new-run/fork, inspection/governance adapters and acceptance
scenarios, or establish P9-03 SDK compatibility.

## Durable new-run execution

Configured execution also supports `turn/start`. Its task must be the server's
selected, unused root identity; its turn ID must be unused. Supply zero expected
and steering revisions and the exact effective cap/currency, request ceiling and
deadline from the trusted execution configuration. Caller objective, constraints
and acceptance text become the canonical task. The host observes the native
repository fingerprint and derives editing/check requirements from the profile.

An optional top-level `root_task` string in the private controller launch frame
selects an absent or existing root before host construction. Workspace/session
and binding still come from the trusted descriptor. Existing selected tasks must
be unredacted roots in that scope. Selection never changes the descriptor, creates
a task, acquires a lease or begins execution. It remains fixed until host shutdown;
observers cannot use this bootstrap override. Relaunch with the same selected root
to inspect its original command and explicitly resume paused work.

Acceptance atomically records the pending root, caller's queued turn, completed
input reference, empty budget ledger and original receipt. Only a fresh accepted
result permits owned retained construction, guarded activation and submission.
The coding loop uses the accepted turn ID. Same-command replay returns before
profile installation or construction; changed payloads conflict. Failure after
acceptance holds work and requests a canonical pause while preserving the receipt;
a failed pause commit remains fenced reconciliation work. The acceptance
response proves durable intent, not successful execution.

The accepted request ceiling is cumulative across root/child/helper attempts and
reopen. Its deadline is absolute from acceptance. A changed profile or later CLI
resume cannot enlarge either limit; shared admission rechecks them even before a
coding loop exists. Suppressed creation evidence prevents admission rather than
removing the limit. Receipt replay can still reconcile a run whose deadline passed.
Legacy CLI roots retain their existing configured-window behavior. See
[ADR-047](../adr/047-durable-public-run-start.md) for the admission contract.

Native Windows/Rust 1.95.0 verification passed 98 engine/protocol tests, 30 live
public-host tests and 91 CLI unit tests. Three compiled-process cases passed:
the new both-store start test plus the existing resume/connected-pause and pipe
owner-loss/reconnect tests. The start case proves exact caller IDs, atomic
Pending/Queued facts, one patch effect, original-command conflict/replay,
budget/controller/observer denial and restart without another provider dispatch.
Host tests also prove expired acceptance remains replayable but cannot authorize
a constructor. The ignored inherited-handle child entry is invoked by its parent
test; the separate real-OneDrive qualification was not run. All 18 fast delivery
checks and affected formatting/diff checks pass. Synthetic literal-loopback
provider fixtures supplied this evidence; no paid provider was contacted.

## Snapshot and event recovery

`session/snapshot` captures typed session/task state and registers an event cursor
at the same canonical sequence. Commits after capture are replayed by `events/next`.
Pages hold at most 128 rows and 256 KiB. A changed source during snapshot pagination
requires discarding partial pages and restarting. Event entries are invalidations
with scoped evidence references, not raw canonical records or complete state deltas.
Clients refresh snapshots to recover current state.

Subscriptions are connection-owned pull cursors, limited to eight per connection
and sixteen per engine. The cursor lifetime is 60 seconds; polling does not renew
it. Each subscription retains only its immediately preceding response for an
identical-token retry. Older, expired, unsubscribed or inaccessible cursors require
resynchronization. Access and retention are checked before cached replies. Logical
retention masks apply even before physical rewriting; gaps and incomplete evidence
are explicit. Slow readers retain bounded pages rather than an unbounded event queue.
Observer disposal releases cursors without fencing writes or pausing the controller.

## Usage and artifact inspection

`usage/read` returns the selected task's root ledger aggregate in exact decimal
USD micros. Settled, reserved and unresolved amounts stay distinct; a missing ledger
is an error. This is a current aggregate: a target may select only that root, and an
inspection cursor is not supported.

`artifact/read` revalidates task scope, current authority and retention, verifies
the full retained hash, and returns at most 49,152 raw bytes encoded as Base64 per
page. Offset and total length count raw bytes; the hash covers the full retained
artifact. `complete` requires both the final range and a complete capture. An
aborted or pending prefix can reach its retained length with `complete: false`.

## Retained context and routing inspection

`context/inspect` lists retained `context-manifest/1` references for the exact
authorized task; `routing/explain` lists recorded `routing-selection/1` references.
They do not assemble new context, rerun routing or imply that a recorded preparation
was submitted successfully. An optional target is an exact artifact ID, not a
guessed turn or attempt association. Clients retrieve bytes through `artifact/read`.

Pages contain at most 128 typed references. Cursors bind the principal, authority,
workspace/session/task, selected view, target, limit, source watermark and retention
epoch. Changes require a fresh query. Retention masks suppress references; absent,
aborted, truncated or redacted evidence keeps `complete` false. Mandatory exclusions
of authentication headers and recovery material are outside the public evidence
contract and do not make an otherwise complete retained reference partial. This
completeness describes the selected retained metadata; full byte hashes are verified
when an artifact range is read.

## Existing workspace lookup

`workspace/open` can observe the binding already selected by trusted launch,
including from an observer connection. Its host and root must exactly match that
binding's host ID and canonical root spelling. The initialize response reports the
same host ID. The method does not resolve caller-supplied paths, create another
writer, establish a new binding, change trust or resume execution.

This existing-binding variant is a read: the schema's `command_id` is unused and
creates no durable receipt. A retry returns current authorized workspace trust and
workspace/authority revisions. Creating or rebinding a workspace still requires its
separate trusted bootstrap workflow; method availability does not grant that right.

The retained-inspection and existing-workspace increment passes 92 engine/protocol
tests, two both-store live-host workspace tests, and two compiled process tests
(inspection/lookup and snapshot replay) on Windows/Rust 1.95.0. Coverage includes
production-shaped capture omissions, genuine missing content, foreign task/schema
denial, exact artifact-byte retrieval, read-only observer lookup and a saved cursor
invalidated by a real intervening canonical commit. No provider is configured.

## Remaining acceptance

The preceding records qualify attachment, owned resume, connected pause,
controller loss, retained inspection and bounded snapshot/event recovery within
their stated test boundaries. Remaining work includes the other method adapters,
pending-input reconnect, process crash after durable acceptance but before response,
abandoned/slow consumer qualification and final CLI/API behavior parity.

Snapshot capture and cursor registration share a canonical worker boundary.
Remaining process-level consumer tests must prove that gaps/disconnects release
resources without blocking writes. P9-03 follows with SDK tests against the
compiled server; P4-01 then attaches the prepared extension UI through that SDK.

## Controlled stdio launch

[ADR-044](../adr/044-controlled-local-process-bootstrap.md) selects a native bridge
so that a TypeScript client does not have to reproduce Windows handle security.
Launch the trusted absolute `vcp.exe` with the sole argument `local-bridge` and
private stdin/stdout/stderr pipes. The first stdin JSON line is:

```json
{"schema":"vcp-local-bootstrap/1","workspace":"D:\\work\\project","data":"D:\\private\\vcp-data","role":"controller"}
```

Use `observer` for read-only access. `data` may be null to resolve the bridge's
local application-data default. The workspace must already have a valid selected
descriptor and binding. The bridge returns a `vcp-local-ready/1` frame with the
authenticated server process pin and authorized scope. Then send the public
`initialize` request. No workspace request runs before that handshake. Startup
failures emit stderr diagnostics and close stdout without a CLI JSONL result.

The server advertises implemented engine reads, controller methods, task/turn
controls, snapshot/event recovery and artifact inspection. Configured execution
adds `session/resume` and `turn/start`. Request the required method capabilities during initialization.
`controller/read` gives scoped revision/generation and ownership classification;
it never discloses another actor's token. `controller/acquire` is explicit,
`controller/release` drains and pauses while leaving the connection readable, and
`controller/recover` explicitly resolves a stale process lease. Authentication,
receipt replay and reconnect never acquire or resume automatically. Read/retry
uses the original durable command identity. Replaying recovery after a later
acquisition returns the original receipt without disturbing that acquisition.

With the default `stdio` transport, the bridge and its server share a lifetime. EOF closes the controlling
connection and allows canonical cleanup before releasing the writer. Another
launch against the same root fails while the first writer remains alive. A slow
or invalid stream cannot grow an unbounded queue: frames have a 1 MiB payload
ceiling, helper queues hold at most eight frames, and blocked output has a five
second deadline. Only one RPC is dispatched at a time. Clients use only the
capabilities advertised by the current adapter; bounded event recovery retains
the acceptance requirements above. The optional pipe transport retains the server
independently after an authenticated bootstrap handoff.

## Named-pipe attachment and reconnect

### Governed memory inspection

The live host advertises `memory/inspect` and `memory/inspection-state/1`. Clients
must negotiate both. The method reads claim/version history under the requesting
actor's current authority and session scope; the requested task supplies the
canonical applicability fingerprint. It uses read-only governed memory access,
without retrieval, generation changes or model requests.

Each finding includes typed `state`: memory sequence and canonical watermark,
retained/pruned/purged visibility, applicability, current-head status, retained
resolution and evidence availability. Pruned/purged lineage remains visible when
authorized, but its removed statement, resolution and evidence content do not.
Disputed versions remain explicit. Available evidence references preserve exact
source ranges and digests. An explicit version filter is applied after claim
authorization; an unavailable version is an error.

History projection has a 128-finding, 64-KiB-per-statement and 256-KiB-page ceiling
and a cooperative two-second deadline. Connection loss or a fenced worker
interrupts it. Overflow is an error rather than a truncated complete history.
The result's `complete` flag concerns the selected history, not evidence truth or
availability. See [ADR-050](../adr/050-governed-public-memory-inspection.md).

Native Rust 1.95 qualification passes 107 engine/protocol tests, three governed
lifecycle tests and the compiled `local_memory_inspection` fixture. Both stores
cover retained/disputed, logically pruned and physically purged histories, exact
source ranges, capability/scope denial and unchanged canonical-state digests.
Actual schema regeneration, strict TypeScript, generator contracts and all 18
fast delivery cases pass.

### CLI and API execution parity

The compiled `local_execution_parity` fixture drives the same patch, verification
and completion scenario through CLI `run` and API `turn/start` on both stores.
It compares typed task/turn semantics, objective and acceptance conditions,
accounting, attempts/reservations/settlements, effects and verification outcomes.
Each run validates its own scoped identity links; random IDs, timestamps and
path-dependent hashes are not equality criteria. Both frontends complete with
three settled synthetic provider requests, 300 micro-USD settled and no active or
uncertain liability. Each independently produces the expected file and passes
the external Node acceptance test. This qualifies that shared execution scenario,
not the remaining method adapters or SDK.

### Retained proposed changes

The live host advertises `diff/read`. Its `change` is the canonical tool-effect
identity, not a path or arbitrary artifact ID. Registered file tools retain a
separate `vcp-public-diff/1` document alongside their private preparation. Event
evidence references make that capture discoverable; its `change` field supplies
the identity for subsequent diff ranges. The capture contains relative paths,
SHA-256 and base64 before/after content, with `disposition: "proposed"`. It is not
proof that the change was applied. Process and MCP proposals without typed file
changes have no diff.

The reader checks current scope, the retained initial `Validated` effect fact,
both capture hashes and exact typed source correspondence before using the
ordinary artifact-range path. A missing, redacted, masked, corrupt or incomplete
source cannot produce an apparently complete diff. Historical proposals lacking
this capture return unavailable. Changes to the live workspace never synthesize
or alter these retained bytes. See
[ADR-049](../adr/049-retained-public-diff-evidence.md).

The compiled pending-input fixture discovers the actual proposal through event
evidence and reads it through both artifact and diff methods. On both stores,
the same proposal bytes survive an independent workspace edit, controller
reconnect and explicit approval denial. Ninety domain/engine tests and all 30
public lifecycle-host tests pass on native Windows/Rust 1.95.0.

### Unreceived receipts and abandoned readers

The compiled `local_crash_receipt` fixture uses a bounded JSON-RPC batch whose
read-result prefix exceeds the measured Windows stdout pipe capacity. It stops
reading the controller output, confirms a final session-create command's durable
acceptance through an observer, then terminates only its own process-pinned server.
After reopening, the original receipt is readable; explicit controller recovery
and acquisition allow a same-command retry without creating another session.
This proves recovery before the client consumes its receipt. It does not assert
that the server had not already sent or buffered the response.

A second case leaves the abandoned connection open. The production blocked-output
timeout releases its controller while the observer remains responsive and the
server remains alive. A replacement controller must explicitly acquire a lease
before committing new work. Both cases pass for Files and SQLite without provider
requests or production failure hooks.

### Pending approval source revisions

Clients that need to answer pending approvals require
`approval/source-revisions/1` during initialization. Negotiated task and snapshot
views expose the approval's observed `effect_revision` and `policy_revision`
alongside its own revision and operation digest. Without negotiation the added
fields are omitted, preserving the strict v1.0 response shape; see
[ADR-048](../adr/048-capability-gated-result-fields.md). These are source facts,
not reusable authority. The response still requires current source and steering
revisions, process ownership and a controller lease for this connection.

The compiled `local_pending_input` test passes on native Windows/Rust 1.95.0 for
both canonical stores. A synthetic loopback provider proposes an actual patch
under a policy that requires approval. Controller loss preserves that pending
input and pauses the task. A replacement connection reads the source counters,
is denied before acquiring a lease, rejects a stale source revision, and explicitly
denies the approval. Repeating the same command returns the original receipt;
an observer can read it but cannot answer. The file remains unchanged, the task
remains paused and provider request count does not increase after disconnect.
The fixture also verifies eventual idle writer release. This test reconnects to
the same live server; it does not treat an earlier process's approval as actionable.
The increment also passes 100 native engine/protocol tests, schema regeneration
and drift verification, strict TypeScript checking and the 18-case fast suite.

### Attachment lifecycle

Add `"transport":"windows_pipe"` to the launch frame to select the pipe service.
The trusted bridge first launches and authenticates the server using the same
inherited-handle bootstrap. The server creates a random local pipe endpoint with
a current-user ACL and rejects remote clients. Before handing off child lifetime,
the bridge authenticates that pipe against the held server process pin, connects
successfully, and exchanges a challenge-bound handoff acknowledgement over the
private bootstrap channel. Failed handoff retains owned child cleanup.

The ready frame adds `attachment`, containing `endpoint`, `server` and a random
`ticket`. A controller-ready frame also includes a distinct `observer_attachment`.
Retain these objects in trusted client memory. Give an observer only the observer
object; its ticket cannot request controller capability. The server does not write
endpoint metadata or credentials into workspace files, discovery files, arguments
or environment variables. Pipe names and PIDs alone are not authentication.

To attach again, launch the same trusted absolute `vcp.exe local-bridge` and send
a `vcp-local-attach/1` frame with `attachment` set to the retained object and `role`
set to `controller` or `observer`. The bridge verifies the kernel-reported pipe
server against the trusted process pin before sending its ticket. The server
reads a bounded authentication frame, then synchronously impersonates the pipe
client, checks the actual client process and token principal, and reverts before
another await. The ticket's own role ceiling governs the new connection. After
the new ready frame, perform public `initialize` again. Fresh authentication gives
a new connection identity; it never acquires a controller lease or resumes work.

Authenticated clients share one canonical host and writer. Disconnecting a
controller still invalidates admission immediately, holds and pauses work, drains
owned effects and releases its lease. Remaining observers continue reading. Once
all authenticated connections and their cleanup have finished, the server retains
its idle writer for 30 seconds to permit reconnect, then tears down. This retention
does not grant execution time after controller loss. If that server has exited,
its old attachment cannot authenticate a replacement; use a fresh trusted launch.
Pipe pumps preserve prefetched authentication bytes and share the stdio frame,
queue and synchronous loss-invalidation limits. Authentication and admitted clients
are independently bounded to 16 concurrent connections each.

Native Windows/Rust 1.95.0 qualification passed 20 local unit tests and eight
compiled production-process tests (five stdio, three named-pipe). The pipe tests
cover both-store competing controllers, observer isolation, explicit reacquisition,
rejected pins/tickets/role escalation, observer-only launch, real 30-second idle
writer release and dead attachment rejection. Native kernel checks include wrong
expected SID/session fixtures; a separate foreign-user process was not exercised.
No credentials, network provider or paid effects are needed. The 18-case fast
delivery suite and affected Rust formatting checks pass. This qualifies the
attachment increment, not the remaining P9-02 execution/event acceptance.

The preceding stdio increment's native qualification uses the compiled production
`vcp` binary in five process
tests: both-store controller receipt replay and explicit release, both-store
observer denial, writer exclusion/EOF cleanup, malformed or unproved bootstrap,
and oversized protocol input after acquisition. They seed actual descriptors and
bindings, remove provider credentials, and verify canonical lease/task state after
process exit. No provider requests or paid effects occur. Separate native tests
check process pin tampering, executable replacement denial, inherited kernel
object identity (including an excluded decoy), bootstrap handle sealing and owned
child cleanup. The ignored child-fixture entry is explicitly launched by the
inheritance test, rather than counted as a deferred qualification.

Transport loss synchronously invalidates the connection and seals owned admission
before notifying asynchronous dispatch. A gated active-stream host test exercises
that interval before `disconnect()` runs; observer and stale loss signals cannot
hold a newer owner. All 11 public-host tests and 71 engine/protocol tests pass on
native Windows/Rust 1.95.0. Generated schemas/types, their nine generator contracts,
and the 18-case fast delivery suite pass. These counts describe the preceding
stdio increment; they do not establish the new named-pipe/reconnect qualification.
