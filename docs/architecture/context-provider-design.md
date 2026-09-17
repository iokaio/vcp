# Context provenance and model-provider design

Status: proposed implementation design for P2-01/P2-02/P2-05/P2-08 with inputs
from P1 capture/accounting and later P6 routing/P7 extensions. Names below are
proposed contracts, not implemented APIs. Product authority remains in
[context and compaction](vcp-what.md#6-context-instructions-and-compaction),
[OpenRouter strategy](vcp-what.md#7-openrouter-and-model-strategy) and
[full capture](vcp-what.md#143-capture-policy-and-truthfulness).

## Boundary and source ownership

Retain qualified Codex context/lifecycle and Gemini discovery components behind
one scope resolver and one model gateway. The assembler produces attributed
content and a reproducible manifest; it neither grants execution authority nor
opens a provider connection. The provider adapter translates that sealed request,
normalizes exposed results and never executes tool calls. The controller and
[engine execution contract](engine-execution-design.md) admit attempts and effects.

Proposed logical responsibilities are `vcp-repository` for roots/versions,
`vcp-context` for instructions/selection/compaction, `vcp-models` for capabilities,
wire conversion/streaming/usage, and the retained `vcp-engine` for orchestration.
P0-07/P0-08 choose concrete modules in the Codex-derived workspace. Keep provider
SDK objects out of memory claims, tool definitions and domain entities.

## Workspace observations and instruction scope

A proposed `WorkspaceObservation` records workspace/root-binding revision,
repository/worktree identity, observed HEAD/base, staged/unstaged/untracked path
sets and content-version references. A Git commit alone is not a fingerprint of
a dirty workspace. Preserve path casing, encoding and line endings in evidence;
normalize only the separate identity/comparison representation appropriate to
the qualified Windows filesystem.

Discovery enumerates explicitly authorized roots using bounded file counts,
depth/size and total-byte budgets. Revalidate resolved links/junctions before
reading; a path's textual prefix is not proof of containment. Record ignored,
binary, generated, oversize, missing and denied inputs with reasons. Absence from
a bounded listing is not proof that a file does not exist. Use filesystem change
notifications as invalidation hints, then verify source identity/content when it
matters; missed watcher events cannot authorize a stale edit.

Instruction resolution returns a proposed `InstructionManifest`:

| Field | Meaning |
|---|---|
| `source_id`, `artifact_id`, `content_hash` | Exact retained instruction version |
| `origin`, `trust_class` | Operating/user/project/active-skill provenance |
| `root_id`, `directory_scope`, `path_applicability` | Which planned file operations the text applies to |
| `precedence`, `discovery_reason` | Why this source was included and how conflicts were interpreted |
| `policy_revision`, `steering_revision` | Enforcement/task context at resolution |
| `conflicts`, `omissions` | Unresolved directions and explicit discovery limits |

For each affected path, walk its authorized instruction ancestry from broad to
specific scope, loading each distinct AGENTS.md version once. Parent instruction
discovery outside a project root needs explicitly allowed instruction-read scope;
it does not authorize general file reads there. Instructions from a nested
directory do not govern unrelated sibling files. A task spanning directories
keeps per-path applicability rather than flattening every instruction into a
global rule. When another path becomes relevant, discover its scoped instructions
before preparing an action against it.

Explicit user constraints retain precedence; trusted capability ceilings remain
outside prompt semantics. Active skill bodies cannot expand grants. Retrieved
memory, source, tool output and MCP descriptions are attributed evidence even if
they contain imperative language. Only AGENTS.md is the initial default; foreign
configuration import stays in P10-02. Do not let two borrowed loaders silently
load CLAUDE.md/GEMINI.md and create duplicate or conflicting instructions.

## Reproducible context contract

Proposed `ContextPart` contains stable source/version ID, trust class, workspace
and access scope, artifact/range references, logical role, mandatory/optional
classification, token estimate/method, inclusion reason and omitted ranges.
`ContextManifest` records ordered parts plus task/turn/step identity, steering,
instruction, policy, authority/deletion, tool/skill, memory-view and repository
revisions. It also records selected capability/catalog snapshot, tokenizer,
reserved output, provider overhead and uncertainty margin.

A proposed `RequestProjection` links a sealed manifest to provider-compatible
message roles, ordered tool-call/result pairs, activated schemas, supported
parameters and body bytes/digest. Role conversion is explicit metadata. An
adapter unable to express a required distinction rejects that projection rather
than silently turning untrusted evidence into a higher-priority instruction.
Treat provider role support and exact payload names as compatibility-tested
properties, not assertions inferred from a model family name.

Artifacts store the original source range and selected view. If a source was
redacted or partially retained, the manifest carries that visibility limitation.
An inspector reproduces the retained transmitted body separately from a
human-readable reconstruction. Prompt reproduction never re-reads today's file
and labels it as yesterday's input.

## Selection and envelope handshake

Use this bounded algorithm for each model step:

1. Read current authoritative task fields and scope. Obtain a candidate model's
   capability envelope and immutable price/catalog/compatibility references.
2. Gather mandatory operating/task instructions, applicable project instructions,
   active schemas/skills, current state and unresolved tool/result pairs. Resolve
   actual content before estimating sizes.
3. Gather optional evidence from exact requested paths/ranges, current diffs,
   diagnostics, lexical/symbol search, recent outputs and authorized memory.
   Record source versions and retrieval freshness. Deduplicate overlapping
   content by source/version/range; keep distinct provenance links.
4. Compute usable input capacity after output reserve, schema/message overhead
   and tokenizer uncertainty. Count through the selected tokenizer when qualified;
   otherwise retain a conservative estimate marker.
5. Fit mandatory parts first. If they alone exceed capacity, return a typed
   context-capability failure or partition work explicitly. Never truncate a
   user constraint, file precondition or required tool pair to make it fit.
6. Rank optional evidence using reproducible features such as explicit path
   relevance, current-change relevance, freshness and source quality. Preserve
   deterministic tie breaks and inclusion/exclusion reasons. Avoid an additional
   paid ranking request in the initial implementation.
7. Serialize the actual candidate request, recalculate its envelope, then hand
   that result to final routing and atomic budget admission. A different model,
   changed schema or smaller fallback restarts fit/serialization/admission.
8. Seal the manifest and request artifact; immediately before send, validate the
   relevant scope/revision fence and the live attempt reservation.

This need not hash every workspace file before each call. Track an explicit
dependency set of included sources, instructions, schemas, grants and roots.
Revalidate that set and relevant access/retention revisions. The engine's send
admission fence orders a revoke/prune change against pending sends: if the change
commits first, the old manifest cannot be sent. If send admission already passed,
cancellation may prevent remaining transfer but cannot retract content already
sent. Record that boundary rather than claiming atomic revocation at a remote
provider. The fence never holds a database transaction over the network.

## Refresh, compaction and handoff

Refresh after an edit, tool result, user steering, instruction/schema update,
root rebind or relevant memory/retention change. Compare dependency revisions;
discard prepared projections whose relevant inputs no longer apply. A model
response from old steering remains captured evidence but cannot silently restore
old constraints or dispatch now-stale effects.

Proposed `CompactionRecord` contains input event range and source hashes, summary
artifact, summarizer attempt or deterministic algorithm/version, authority and
deletion revisions, preserved-reference manifest and visibility limitations.
Keep objective, accepted constraints, steering, grants/denials, current diffs,
open questions, pending calls, unknown effects, reserves and required checks as
authoritative fields outside the summary. Summarize descriptive history only.

Compute a compaction candidate from a coherent source view, reserve a helper
attempt when it uses a model, validate required references, then commit its
projection if dependencies remain current. Rebuild or reject on a conflicting
steering/retention change. Failed compaction leaves the prior valid projection
and original history intact. Record input/output size and useful capacity gained;
stop a repeated no-gain loop. Protect verification/reporting funds before buying
summarization. A summary is generated evidence, not automatically accepted memory.

Prune/revoke invalidates summaries derived from affected sources as well as direct
source chunks. Rebuild from still-authorized inputs or mark unavailable; a cached
summary cannot retain deleted facts invisibly. Full historical capture stays
subject to its explicit retention/access policy, not to prompt compaction.

Proposed `HandoffPacket` records objective/constraints, scope, diff/base and file
versions, decisions/evidence, open work/effects, checks, budget view and tool
pairing metadata. The destination assembler selects compatible content under its
own model envelope. A tool result is never emitted without its matching call in
the serialized conversation; if a provider cannot carry the old pairing, use an
explicit attributed summary representation and record the conversion. Omit
unsupported opaque provider fields with a reason; do not invent missing reasoning.

## Provider admission and request translation

Only the controller's admitted attempt enters `ModelGateway`. Proposed request
fields include attempt/root/task/step/role IDs, reservation receipt, capability
snapshot, manifest/request-artifact IDs, provider restrictions, model pin/fallback
policy, output/effort bounds, deadline and cancellation identity. The gateway
checks that the immutable serialized request matches the admitted identity.
Credentials are resolved separately in the trusted host and excluded from this
serializable envelope.

P2-02 maintains a capability conversion table: normalized feature, provider field
mapping, supported values, incompatible combinations, evidence fixture and
observed revision/date. Unknown required features fail before submission. An
optional unsupported parameter may be omitted only through a documented
normalization rule recorded on the attempt; never silently strip a required tool
or data-policy constraint.

Begin with one qualified model and explicit provider constraints. Disable or
constrain transport/SDK automatic retries and gateway fallbacks until their
possible requests fit the same accounting, context and attribution contract.
VCP-controlled retries create new attempts. A transparent upstream fallback is
eligible only if the candidate set shares the request's required capabilities,
context bounds and data policy, its billing can be accounted for, and served
identity/uncertainty can be reported. Otherwise require VCP-side reassembly and
admission or report unsupported behavior. A model pin without authorized fallback
must remain a pin.

Capture and commit the normalized request/manifest and reservation, then record
submission intent before transport sends. A timeout after that point is
potentially billable. Catalog refresh is a new immutable snapshot; it never
reinterprets the price or capability evidence on a live attempt.

## Streaming and usage normalization

Implement transport framing, payload parsing, normalized event conversion and
controller consumption as separate bounded stages. Test byte splits inside
UTF-8 code points and JSON strings, combined frames, empty keepalives, malformed
lengths/events, truncated terminal messages and interleaved tool calls. Spool
exposed raw response content locally with authentication headers removed.

Maintain a proposed accumulator per `(attempt_id, provider_call_id)` with tool
name, ordered argument fragments, completion flag, size counters and parse error.
Display partial arguments as incomplete data. A tool call becomes execution
eligible only after complete validated JSON, a matching known tool schema, a
unique normalized call ID and a recorded containing response boundary under the
qualified adapter contract. Partial text that happens to parse at one moment is
not a completed call. Cap per-call bytes and total concurrent accumulators.

The initial loop may wait for the response terminal boundary before admitting
complete tools; early tool dispatch requires separate qualification of terminal,
cancellation and retry behavior. Preserve response order independently from
parallel worker completion order, correlating each result to its original call.
An adapter never dispatches a tool while parsing a chunk.

Normalize requested model, served model/provider and provider request ID as
separate nullable/unknown fields. Do not fill served identity from a request
assumption. A proposed usage observation carries source identity, completeness,
currency, raw fields and normalized disjoint charge categories. Distinguish
cumulative updates from incremental deltas. Duplicate terminal frames cannot
settle twice; conflicting terminal/usage observations remain evidence and trigger
reconciliation. EOF without a qualified terminal marker is incomplete, even if
some answer text arrived.

Cancellation stops scheduling further work and asks the transport to stop.
Persist retained output and submission certainty, keeping reserves when billing
is unknown. Do not wait indefinitely for final usage; late usage is an idempotent
reconciliation event. Buffer exhaustion or capture failure cancels/pauses through
the same observable path, not an invisible dropped-chunk policy.

## Error classification and bounded retry

| Class | Controller behavior |
|---|---|
| Invalid envelope/schema/capability before send | No request; expose actionable configuration/context failure and release only proven undispatched reserve |
| Local connection failure with positive no-send evidence | Record attempt failure; bounded new admission may retry |
| Rate limit/transient transport failure | Respect qualified delay/deadline policy; new attempt, with old liability retained where uncertain |
| Partial response or ambiguous timeout | Retain partial output and unresolved accounting; do not replay tool effects |
| Provider rejects model/data policy | Refilter only within authorized fallback; otherwise block and explain |
| Malformed tool call | Record invalid response/call; no broker dispatch; bounded repair is a separately admitted model attempt |
| Unsupported or missing usage | Explicit unknown category/reserve; qualified reconciliation or explicit accounting resolution |

Retry state contains attempts used, reason, next eligible time and task deadline.
Apply an upper bound across VCP and adapter retries, and cancel timers on pause.
Do not sleep while holding controller/store locks. A stream error after a tool
effect does not authorize that effect to run again. Provider API shapes, usage
semantics and reconciliation endpoints must be revalidated against primary
OpenRouter documentation during P2-02; this design intentionally does not freeze
an unverified endpoint or SDK version.

## Integration and acceptance

Use a scripted transport and a disposable repository to drive context selection,
serialization, admission, response parsing, prepared tools and fresh verification
through the retained engine. Observe request count/reservation IDs independently
of VCP's own reported counters. Compare manifests against small explicit fixtures,
not exact model prose or an implementation-generated expected snapshot.

| Task | Fixture and observable result |
|---|---|
| P2-01 | Nested/sibling instructions, junction escape, generated/large files and dirty Git state yield correct scopes and exclusions without modifying the repository |
| P2-01/P2-02 | Larger schema or smaller fallback forces reassembly; every transmitted request fits its admitted envelope |
| P2-02 | Split/interleaved/malformed stream emits no incomplete tool dispatch; duplicate usage settles once; missing usage remains unknown |
| P2-05 | User correction during streaming invalidates affected prepared operations while late output remains inspectable |
| P2-08 | Long trace compaction preserves latest constraints, outstanding costs/effects and original retained artifacts |
| P2-08/P5-07 | Revocation/prune before send admission blocks the affected manifest, including derived summaries; prior admissions retain submission certainty and best-effort cancellation |
| P6-03 | Model handoff preserves tool pairing or records explicit conversion, scope and remaining budget |

Run E02/E04/E11/E12/R05/R07 and the relevant U01/U03/U07 scaffolds through the
[shared test plan](../plan/16-test-fixtures-and-acceptance.md). Live provider
qualification is separately capped and authorized. Tokenizer availability,
uncertainty margins, stream termination rules, compatibility evidence freshness
and compaction thresholds remain measured choices in P2/P6; missing measurements
must not be reported as shipping support.
