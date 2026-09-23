# Public protocol construction reference

Owning item: P9-01. Status: v1.0 protocol and bounded engine adapter accepted and
delivered in [PR #134](https://github.com/iokaio/vcp/pull/134). No local server or SDK support is
declared. [ADR-042](../adr/042-owner-directed-p8-closure.md) opens P9
after owner-directed P8 closure; it does not qualify the API. The method names
below come from [architecture section 5.3](../architecture/vcp-what.md#53-minimum-methods).
Implementation and qualification remain governed by
[P9-01](../plan/17-deferred-api-and-sdk.md#p9-01--public-protocol) and the
[client design](../architecture/deferred-clients-design.md).

## Contract versus available capability

A registered schema describes a method's wire shape. It does not establish that
the attached engine can execute that method. Initialization must advertise only
capabilities actually implemented and available under the selected execution
host and configuration. A matching major version alone does not grant access or
imply a capability exists. A known but unavailable operation returns a structured
capability error before mutation; an unknown method follows the selected JSON-RPC
method-error contract.

`vcp-engine::rpc::RpcSession` currently supports exactly `session/create`,
`session/read`, `session/list`, `turn/steer`, `approval/respond` and `command/read`.
Its declared capabilities are the enabled method names plus `jsonrpc/2.0` and
`durable-command/1`; configuration can remove methods but cannot add handlers or
unimplemented capabilities. Generic handshake fixtures may negotiate other
declared capability strings to exercise negotiation; those fixtures do not extend
`RpcSession`'s implementation.

The schema deliberately covers more methods and result shapes than this adapter
executes. Remaining lifecycle/inspection adapters belong to the shared engine and
P9-02 attachment work; editor observation/reconciliation also requires its owning
P4 integration. No public transport exists yet. Native adapter parity tests do not
establish Windows pipe authentication, process ownership or compiled-server SDK
support.

## Optional controller capabilities (P9-02)

The typed registry also defines `controller/read`, `controller/acquire`,
`controller/release` and `controller/recover`. Each is a separate optional method
capability. A live host must explicitly implement and advertise it; the direct
engine adapter continues to advertise only its six existing methods. Initialization
checks configured methods against both the typed registry and the selected host.

All four methods carry the existing workspace/session `scope`. Acquire carries a
durable `command_id` and optional `expected_revision`: null or absence means no
previous lease; a decimal revision identifies an existing released lease. Release
and recover carry `command_id`, exact decimal `expected_revision` and positive
`generation`. Recovery is explicit stale-process cleanup, never takeover of a live
owner. These operations do not use a task steering revision. Unknown fields,
including actor, connection, token or write claims, are rejected. Authentication
supplies the principal and connection; initialization cannot grant ownership.

Mutations return the existing durable `acceptance` result. `controller/read`
returns `kind: controller` with scope, nullable lease revision, generation, watermark
and ownership relative to this authenticated connection: `unclaimed`,
`this_connection`, `other_connection`, `previous_process` or `released`. An absent
lease has null revision and generation zero. This view is observation, not a token.
Reconnect does not inherit the previous connection's lease. Reconcile an uncertain
mutation through its original command identity; never infer ownership from an old
acceptance. Canonical lease commands retain the engine's connection/process-bound
identity rather than accepting a caller-supplied digest.

Acquisition never resumes work. Release must hold, pause and drain the owned work
before removing ownership, while leaving observer reads available. Current access
is checked before receipt replay. Controller generation and revision are checked
again when a decision or other mutation is admitted. Snapshot/event delivery and
compiled endpoint authentication remain separate P9-02 acceptance requirements.

## Canonical definitions and generated artifacts

The Rust wire DTOs in `vcp-protocol/src/methods.rs`, handshake, errors and JSON-RPC
modules are canonical. With the `schema` feature, schemars 0.8.22 derives draft-07
JSON Schema from the `PublicApi` bundle. The `vcp-protocol-schema` executable emits
that schema to stdout. `scripts/protocol/generate-types.cjs` uses only Node standard
library APIs to produce `src/packages/protocol-ts/schema.json`, `index.ts` and
`package.json`. These are generated wire definitions, not a runtime SDK.

The exporter embeds hashes of the Rust sources, its own source, the protocol
manifest and retained workspace lockfile as a JSON string in `$comment`. Hashes
cover normalized LF source bytes captured by the compiler. Running an old binary
after editing a DTO cannot refresh that provenance. The fast generator test checks
the exact source list and hashes before checking all generated outputs for drift.
Native regeneration remains required to establish the semantic schema output.

The generator rejects unsupported schema structure, unresolved references and
unsafe TypeScript names. Value constraints remain in JSON Schema and runtime
validation: TypeScript alone cannot enforce arbitrary patterns, exact integer
ranges, extra-property rejection or exclusive `oneOf` branches. Request DTOs also
apply runtime semantic validation; generated types are not an admission guard.

Regenerate from the repository root in PowerShell 7 with the native Visual Studio
developer environment and Rust 1.95.0 available. Cargo runs in the retained
workspace so its `.cargo` configuration applies:

```powershell
$repoRoot = (Get-Location).Path
$schemaFile = Join-Path $repoRoot 'artifacts/p9-schema.json'
New-Item -ItemType Directory -Force (Join-Path $repoRoot 'artifacts') | Out-Null
$env:CARGO_TARGET_DIR = Join-Path $repoRoot 'artifacts/codex-target'
Push-Location (Join-Path $repoRoot 'src/third_party/codex/codex-rs')
try {
    cargo +1.95.0 run --locked --offline --quiet -p vcp-protocol --features schema --bin vcp-protocol-schema |
        Set-Content -LiteralPath $schemaFile -Encoding utf8NoBOM
    if ($LASTEXITCODE -ne 0) { throw 'Native schema generation failed' }
} finally { Pop-Location }
node scripts/protocol/generate-types.cjs $schemaFile
node scripts/protocol/generate-types.cjs $schemaFile --check
node --test src/tests/contracts/protocol-generation.test.cjs
```

The executed native wrapper was `scripts/evals/p8-native-command.ps1` with
`-CommandFile artifacts/p9-schema-command.json -RustToolchain 1.95.0`, using the
same workspace, Cargo arguments and target directory. Its command specification
and stdout artifact are local run artifacts, not inputs required in a clean
checkout. `--offline` requires the pinned dependencies already cached; missing
dependencies must be obtained through the normal repository setup rather than
silently changing versions.

For a fast read-only check against the committed schema, run:

```powershell
node scripts/protocol/generate-types.cjs src/packages/protocol-ts/schema.json --check
node --test src/tests/contracts/protocol-generation.test.cjs
```

The second command also detects Rust-source drift; checking generated TypeScript
against an old schema alone would not detect an edited canonical DTO.

## Common scope, authority and mutation rules

Every workspace operation binds the authenticated principal and execution host to
an authorized workspace. Session, task, turn, artifact and claim identifiers are
selectors, not credentials. Reads recheck current authority and retention, including
reads of old mutation receipts. The adapter must never construct permissive access
merely because a client supplied a valid identifier.

The table uses **O** for an authorized observer or controller read, **C** for a
current controller mutation, and **B** for authenticated workspace bootstrap. B
does not authorize executable workspace content. Controller status does not grant
arbitrary tool, memory-governance or disclosure permission; those checks remain
inside their owning services. `session/export` additionally requires the selected
capture/disclosure policy even when its source session is readable.

All durable mutations carry a caller-owned `command_id`, applicable scope,
expected revisions and a method-specific payload. A connection's JSON-RPC request
ID is separate and may change on retry. Persist a canonical payload digest and
result/operation reference. Under current access, same scoped identity and payload
returns the original result; changed payload conflicts. Do not dispatch again after
an uncertain reply. Receipt acceptance means a durable command record exists,
not that a model, tool, cancellation drain, deletion or export finished.

Internal `CommandEnvelope::digest` covers the entire private envelope. The public
adapter instead supplies a stable digest over `vcp-public/1.0`, authenticated actor
and canonical public call to the same canonical command handler. Controller epoch
is host authority, not part of the public call or its durable identity. Current
access and scope checks precede persisted receipt lookup; same-key retries resolve
before stale preconditions are checked. New commands use the current engine owner.
The existing canonical store commits command receipt and effects atomically;
there is no connection-local replacement deduplication ledger.

For new `session/create`, the requested configuration revision must match the
authorized source session; internal create requires zero expected/steering
revisions and a distinct new session ID. Creation does not grant access to the new
session. For `turn/steer`, the task and nonterminal turn must share the authorized
scope and expected steering, followed by normal task revision admission. For
`approval/respond`, the pending approval must match task/session/workspace and the
current policy revision supplied by both request and host; internal admission also
checks operation digest, effect revision and authority. Duplicate receipt retrieval
does not reapply those operations.

Shared `Engine::query` is read-only. Its access grant names one session:
`session/list` returns only that session, not every session in the workspace.
Cursors bind principal, workspace/session, authority, deletion epoch, watermark
and limit. `command/read` proves session scope from retained correlated events;
missing/pruned scope evidence makes the result unavailable. This prevents a
workspace-scoped receipt key from becoming workspace-wide read authority.

Host facts and access are trusted inputs supplied outside serialized parameters.
`vcp_engine::Access` and `HostFacts` are not client authority assertions. Controller
generation, operation/policy revisions and pending decision must be verified
atomically when answering a decision. Observer replay of a controller command ID
does not grant mutation rights.

## Method inventory

Paths in this table are relative to `src/crates/`. **Durable** means the mutation
identity rules above apply; **read** requires no new durable mutation. Limits are
resolved in the following section rather than inferred from a method's name.

| Method | Scope and role | Preconditions and identity | Acceptance/result and existing internal mapping | Missing shared behavior |
| --- | --- | --- | --- | --- |
| `workspace/open` | Host and canonical root; B for new binding, O for existing authorized workspace | Existing binding/authority revision; durable if creating canonical state | Workspace identity, trust, policy and repository state; `vcp-protocol::Command::Initialize { binding }`, CLI binding/settings discovery | Shared authenticated root discovery/open facade; reopening must not grant trust or resume work |
| `session/create` | Authorized workspace/source session; C | Matching source configuration revision, zero create/steering revisions, distinct new session; durable | Implemented by `Engine::handle_public` and `Command::CreateSession`; durable new identity | P9-02 must supply authenticated owner/access; new session is not automatically authorized |
| `session/read` | Workspace/session; O | Current scope and retention; read | Implemented by `Engine::query(Query::Session)` and `RpcSession`; snapshot with watermark | No broader session authority inferred |
| `session/list` | Authorized workspace/session; O | Bound cursor, authority, deletion epoch and watermark; read | Implemented by `Engine::query(Query::Sessions)` and `RpcSession`; at most the one session visible to this grant | Workspace-wide discovery needs an explicit separate access contract |
| `session/resume` | Workspace/session and affected task tree; C | Expected task/lifecycle revisions, current owner, policy, fingerprint and reconciled effects; durable | Accepted explicit resume and subsequent state events; `CanonicalHost::resume`, `Lifecycle::resume`, CLI `terminal::prepare_resume` | Shared orchestration of those steps and durable retry result; reconnect must not call resume |
| `session/fork` | Source workspace/session/recorded turn and new session; C | Completed boundary, source visibility, new profile/budget authority; durable | New ancestry-linked session/task; `CreateSession { fork_through }`, `CreateTask { fork_origin }`, CLI `app/execute.rs`, worker `coding/fork.rs::fork_history` | Recoverable shared workflow across creation steps; authority, approvals and liabilities must not be inherited |
| `task/read` | Workspace/session/task; O | Current scope and retention; read | Shared nonmutating `Engine::query(Query::Task)` exists; canonical task state | Typed public task projection is not advertised by `RpcSession`; retain pending/unknown detail |
| `task/cancel` | Workspace/session/root task and descendants; C | Expected task/steering revisions and live owner; durable | Durable cancellation receipt, then observable drain/reconciliation; `CanonicalHost::control_envelope` and `stop` in `foundation/worker/control.rs` | Public operation/result lookup and lease binding; do not replace coordinated stop with bare `Transition` |
| `turn/start` | Workspace/session/task/root budget; C | Profile/config, task, steering, policy and budget admission; durable | Durable task/turn acceptance followed by events; `CreateTask`, `StartTurn`, `CanonicalHost::begin_coding_turn`, CLI `Session::start` | Shared recoverable execution setup; record-only start must not be presented as retained-controller dispatch |
| `turn/steer` | Workspace/session/task/turn; C | Nonterminal turn in task scope, expected steering/task revisions; durable | Implemented by `Engine::handle_public` through `Command::Steer`; durable guidance acceptance | Acceptance does not claim model consumption; execution-host integration remains separate |
| `turn/pause` | Workspace/session/task/turn; C | Expected task/steering revisions and live owner; durable | Durable pause receipt; inspectors remain usable while work drains; `CanonicalHost::stop` | Explicit turn-to-task/tree semantics and lease binding |
| `turn/cancel` | Workspace/session/task/turn; C | Expected task/steering revisions and live owner; durable | Durable cancel receipt and retained unknown/partial outcomes; `CanonicalHost::stop`, internal turn transitions | Define affected task/tree semantics; a terminal acknowledgment cannot erase unknown effects |
| `approval/respond` | Workspace/session/task/approval; C | Pending decision, operation digest, effect/policy/steering/authority/binding revisions and current engine owner; durable | Implemented by `Engine::handle_public` and `Command::Decide`; atomic acceptance/stale error | P9-02 attaches authenticated controller lease generation to host admission |
| `command/read` | Workspace/session/command; O | Current access and retained correlated event proof of session scope; read | Implemented by `Engine::query(Query::Command)` and `RpcSession`; durable receipt/result | Pruned scope evidence returns unavailable; lookup does not grant retry or mutation authority |
| `events/subscribe` | Workspace/session; O | Authorized after-sequence cursor and event schema; read/subscription identity | Ordered pages/notifications or explicit gap; `Engine::subscribe/events/unsubscribe`, `CanonicalHost::subscribe_events/events/unsubscribe_events` | Snapshot-plus-live handoff, bounded push queues, connection disposal and persistent reconnect semantics |
| `artifact/read` | Workspace/session/task/artifact; O | Current scope, visibility, retention and bounded byte range; read | Access-checked content/reference; `vcp_audit::inspection::inspect` with `RangeRequest`, `History::read_artifact` | Public range result; do not use whole-buffer `CanonicalHost::read_artifact` as an unbounded public response |
| `diff/read` | Workspace/session/task/change artifact; O | Current scope, change identity and bounded range; read | Stored change content/reference; effect/artifact records and repository tooling | Dedicated shared change-to-artifact lookup and typed diff projection; no arbitrary filesystem read |
| `context/inspect` | Workspace/session/task/step; O | Current scope and snapshot cursor; read | Stored manifest evidence and explicit gaps; `CanonicalHost::inspect`, audit `View::Context` | Typed public projection and step linkage |
| `routing/explain` | Workspace/session/task/step; O | Current scope and snapshot cursor; read | Stored routing decisions/evidence; audit `View::Routing` | Typed public projection; do not synthesize a replacement explanation as authority |
| `usage/read` | Workspace/session/task/root budget; O | Current scope and snapshot cursor; read | Complete settled/reserved/unresolved cost view; audit `View::Costs` | Typed public accounting projection preserving distinct liabilities |
| `memory/query` | Workspace and authorized memory/task scope; O | Current access/view, generation/sequence and bounded query; read | Evidence/findings with generation and sequence; `vcp_memory::retrieval::query`, `CanonicalHost::query_memory_context`, local-memory APIs | Select shared interactive query/resource semantics; do not treat context preparation as an interchangeable public query |
| `memory/inspect` | Workspace/claim/version; O | Current memory access and retained version; read | Claim/provenance/visibility state; `CanonicalHost::inspect_memory`, memory inspection/history APIs | Typed claim/version projection |
| `memory/propose` | Workspace and authorized claim scope; C | Governance authority and source/view versions; durable | Durable proposal/governance status; `vcp_memory::repository::propose` | Uniform shared command/receipt facade, not adapter-owned governance |
| `memory/resolve` | Workspace/proposal/claim version; C | Expected proposal/claim and governance revisions; durable | Durable resolution with explicit rejection/conflict; governance `gates::evaluate` and repository components | Shared authenticated resolution operation; evaluating gates alone does not commit a resolution |
| `memory/forget` | Workspace and selected claims/history/artifacts; C | Current preview/digest, deletion/authority revisions and deletion authorization; durable | Durable staged deletion status; retention `preview/save_preview/apply/cleanup` | Public shared workflow and reconciliation; accepted deletion does not imply physical cleanup finished |
| `editor/context` | Workspace/host/document; C | Document version/hash, observation revision and binding; durable for accepted canonical context | Accepted bounded editor observation; prepared filesystem tooling supplies related primitives | Shared document observation/invalidation behavior is missing; do not advertise capability from schema alone |
| `editor/changeResult` | Workspace/task/prepared change/document; C | Prepared operation, document/binding versions and expected effect revision; durable | Reconciled applied/partial/unknown editor effect | Shared editor-receipt reconciliation is missing; a client success assertion is not verified effect evidence |
| `session/export` | Workspace/session plus explicit capture scope; C | Current visibility/retention/redaction policy; durable | Export artifact and visibility manifest; history/artifact/capture components | Dedicated shared scoped export operation; encrypted backup is not an equivalent API |

## Limits and encodings

The public codec has a 16 MiB absolute frame ceiling; the configured/negotiated
limit may be lower. Incremental UTF-8 line framing rejects invalid encoding and
oversized input before allowing unbounded buffering. The public method-input
ceiling is 256 KiB, pages are limited to 128 items and artifact ranges to 64 KiB.
Shared query results are also bounded to 256 KiB. `RpcSession` bounds batches to
64 entries and checks encoded output against the configured frame limit. A
response-size failure after commit does not undo the mutation or authorize a new
command identity.

The existing internal command ceiling is also 256 KiB
(`vcp-protocol/src/version.rs::MAX_COMMAND_BYTES`). Internal event pages allow
128 events; the engine retains at most 16 pull subscriptions, each with a
60-second snapshot lifetime. Public streaming is not implemented by this adapter;
these internal subscription limits do not imply that it exposes live events.

Audit artifact ranges allow 64 KiB (`vcp-audit/src/inspection.rs::MAX_RANGE`).
The audit inspection page ceiling is 512 KiB, and the CLI session query separately
caps output at 256 records/768 KiB. A public frame limit smaller than these outputs
requires bounded paging/projection; it must not truncate a JSON response silently.
Canonical DTO definitions and runtime `Call::validate` specify the per-method
query text, input, page and range bounds. Future adapters must bound total results
before exposing their capabilities.

Opaque IDs and all 64-bit counters use strings. Domain revisions already enforce
canonical unsigned decimal strings (`vcp-domain/src/revision.rs`); preserve that
behavior beyond JavaScript's safe integer boundary. Public `Counter` accepts exactly
`"0"` through `"18446744073709551615"`, without signs, leading zeros or trailing
line terminators. Money retains exact ledger
representations. Required scope, authority and governance semantics reject unknown
fields/enums rather than defaulting to permission or success. Define absent versus
null per field; presentation additions may be ignored only where the compatibility
contract explicitly permits them.

## Acceptance and failure semantics

Protocol, event-schema and generated-schema versions are currently `1.0`.
Negotiation selects the server's supported `1.0` surface from compatible major-1
peers; a higher minor version does not silently enable a method. Golden negotiation
and codec traces are in `src/tests/fixtures/protocol/v1.json`.

| Peer request | Outcome |
| --- | --- |
| Canonical `1.0` or a newer major-1 minor | Negotiate `1.0`; intersect advertised/requested capabilities |
| Required capability absent, even at `1.0` | Explicit capability failure |
| Different major version | Unsupported-version failure |
| Malformed, noncanonical or out-of-range version components | Invalid parameters |
| Request before initialization or repeated initialization | Explicit initialization-state failure |

New optional presentation fields require an explicit compatible schema policy;
unknown required capabilities and authority/governance enums fail closed. A wire
breaking change requires a major-version contract rather than optimistic decoding.

The v1.0 profile separates bounded frames, JSON-RPC envelope validation,
initialization/negotiation, scope authorization and method admission. Envelopes
require `jsonrpc: "2.0"` and reject unknown fields, conflicting request/result/error
shapes and duplicate keys. Request IDs may be strings, explicit null, or exact
integers from -9,007,199,254,740,991 through 9,007,199,254,740,991. Strings are
recommended. Explicit null is an ID; an absent ID is a notification.

`RpcSession` intentionally ignores valid notifications, including initialize and
mutation notifications: no execution and no reply. An all-notification batch
produces no response frame. Invalid messages get their applicable envelope errors;
an empty batch is Invalid Request, and a batch above 64 entries is rejected before
dispatch. This is a bounded JSON-RPC application profile, not a promise to execute
every valid JSON-RPC notification. Diagnostics never share protocol stdout. Known
methods whose capabilities were not negotiated cannot reach internal execution.

Pending input, denied/stale authority, partial effects, retained unknown outcome,
interrupted transport and terminal engine results are different typed states.
Expose durable operation/command lookup so a disconnected caller can reconcile
before retrying. Cancelling a local await or subscription only stops that consumer;
task/turn cancellation is its own durable mutation.

Subscription reconnect must establish a canonical snapshot sequence and replay
after that exact boundary. Existing pull cursors bind workspace/session, end
sequence, watermark, authority, deletion epoch and expiry, and report explicit
gaps. New live delivery must preserve those guarantees under slow readers and
revocation. An observer disconnect changes no task ownership; controlling-owner
loss invokes the existing close-to-pause behavior. Status reads and reconnects do
not resume paused work.

Local P9-01 verification passed native protocol/adapter tests, the eight generator
contracts, TypeScript checking and thirteen independent schema fixtures. Coverage
includes malformed/split/oversized frames, old/new peer negotiation, unsupported
versions, absent capability at matching version, duplicate mutation/restart
semantics, stale approvals, scoped reads and independent large-counter boundaries.
Internal-command/API parity applies to the implemented adapter methods, not every
registered method schema. Source provenance and generated-output drift are checked
without pretending a JavaScript type check executes Rust schema generation.

P9-02 still requires real authenticated local processes, controller leases,
owner-loss/pause, snapshot/live-event handoff, bounded subscribers and restart
qualification. P9-03 requires the SDK against a compiled real local server,
including retry, cancellation, disposal and gap recovery. These future tests cannot
be discharged by P9-01's in-process adapter or synthetic schema fixtures.

## ACP consideration

An ACP adapter was considered as an additional client surface. The native VCP
contract remains necessary here because the selected acceptance contract exposes
VCP task/root-budget identity, retained unknown liabilities, revision-bound trust
and governance, controller ownership and canonical event recovery. This increment
does not establish an ACP mapping or claim ACP cannot represent those concepts.
Keep an optional ACP compatibility adapter deferred until a concrete client need
justifies a separate mapping and qualification task. Do not replace the agreed
native method contract or introduce another controller to pursue that option.
