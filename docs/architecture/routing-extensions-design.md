# Routing, extension and delegation implementation design

Status: proposed implementation design. This document expands the contracts owned
by [routing](../plan/12-routing-and-optimization.md),
[skills and MCP](../plan/13-skills-and-mcp.md),
[delegation](../plan/14-visible-delegation.md) and the deferred
[extension tasks](../plan/19-deferred-extensions-and-platforms.md). It records no
implemented feature, measured model default, selected upstream revision or accepted
ADR. The owning requirements remain [architecture sections 7–8](vcp-what.md#7-openrouter-and-model-strategy),
[15](vcp-what.md#15-instructions-skills-hooks-and-mcp) and
[16](vcp-what.md#16-multi-agent-work-and-integration).

## Service boundaries and persisted identities

Use opaque domain IDs and references to canonical artifacts. The field names below
are implementation proposals to refine against P1's contracts, not a second wire
schema. Amounts use the ledger's exact monetary representation, never floating
point. Each mutating command carries its normal command identity, expected revision
and causation; rejected stale work remains explainable without taking effect.

| Record | Proposed contents | Mutation or authority boundary |
|---|---|---|
| Catalog revision | Source observations, fetched time, effective time if supplied, exact model/provider identities, capability states, price units, availability | Immutable snapshot; refresh changes the current pointer after validation |
| Compatibility observation | Catalog/model/provider/config revision, capability, observed result, fixture/run refs, sample count and limitations | Evidence can override an optimistic capability claim only through reviewed compatibility rules |
| Routing policy revision | Workspace, parent revision, role/group allowlists, exclusions, pins, preference ordering, quality requirements, effort/output bounds, escalation/concurrency limits | Selected configuration changes; effective values remain under trusted ceilings |
| Routing decision | Task/step/role, input fingerprint, policy/catalog/evaluation refs, ordered candidates, exclusion reasons, cost assumptions, chosen model/provider constraints | Pure selection until context assembly and ledger admission complete |
| Extension activation | Source/package identity, version/content hash, component, activation reason, workspace scope, permitted capabilities, manifest refs | Content inclusion; it does not mint an execution capability |
| Child assignment | Parent/root IDs, objective, dependencies, scope, base snapshot, allocation, depth, deadline, acceptance and authority revision | Root controller registers before any child work is scheduled |

Separate computation from effects. `select(input, snapshots)` returns a decision
proposal; `assemble(decision, context)` returns an actual bounded request;
`admit(request, expected_policy)` invokes the shared ledger and policy boundaries.
Extension validation returns a prepared tool operation; only the existing broker
can dispatch it. Child scheduling invokes those same services. Avoid holding a
canonical transaction across discovery I/O, model calls, subprocesses or user input.

## Catalog and routing decisions

Represent capability facts as `supported`, `unsupported` or `unknown`, with source
and observation time. A missing tool flag or price is not false or zero. Keep
observed provider restrictions distinct from VCP's trusted data policy. An entry
whose required price units cannot be normalized is unavailable for new admission
until reconciled; retain it for history and explanations.

An exact identity may still name a mutable provider alias. Record that limitation
and the served identity when exposed; do not claim an immutable model revision when
the gateway does not supply one. Catalog aliases are lookup aids, not evidence that
a renamed model has the old model's evaluations. Group membership and measured
role suitability have their own revisions so a metadata refresh does not silently
rewrite an evaluation's meaning.

Run this deterministic selection sequence:

1. Capture workspace, task/steering revision, role, current pin/fallback permission,
   capability/context requirements, effective policy and catalog/evaluation refs.
2. Resolve allowed candidates by exact identity. Apply workspace/data/provider
   restrictions, available capabilities, context/output bounds and price knowledge.
   Record stable reason codes with evidence for exclusions, including strict-pin
   failure. Do not quietly unpin when the permitted set becomes empty.
3. Apply the task-class quality floor using qualified evaluations. Insufficient
   evidence is a distinct condition; it cannot become a perfect score. A configured
   fallback pool may use a previously qualified broader class with the substitution
   disclosed. Otherwise report the missing qualification or require a bounded trial
   authorized separately from routine work.
4. Rank eligible candidates using the policy's explicit quality/cost/latency order
   and a final stable exact-identity tie break. Preserve the values and comparison
   order, including confidence/sample limits; do not hide an unexplained weighted
   score behind a capability group.
5. Assemble context for the selected capability envelope. Validate actual message,
   schema, input and maximum-output bounds. If assembly cannot fit, record why and
   try only a bounded next candidate or a permitted context reduction. Do not drop
   required authority, unresolved effects or task constraints to fit a cheaper model.
6. Compute the reservation using the assembled request and gateway charge mapping;
   atomically admit it under root/child/session/global limits and protected reserves.
   A concurrent admission failure reruns the budget-dependent decision or stops.
   Selection never reserves spend by subtracting from a stale local balance.
7. Persist decision, context and attempt/reservation linkage before dispatch. Inspect
   actual served attribution and compatibility after response; record differences
   without retroactively changing the original decision.

Expected total task cost is a ranking estimate, separate from the next-attempt
reservation ceiling. Its components include first attempt, observed retries,
handoff/context rebuild, support roles, children and protected completion work.
Keep each component's assumptions and sample basis. Use a pessimistic documented
bound when a distribution is unavailable, or expose the candidate as unqualified;
never use an invented zero-cost retry path. Gate unknown invoice liabilities using
[the reservation state machine](vcp-what.md#82-reservation-state-machine).

## Handoff and escalation barriers

An escalation record binds trigger class, triggering evidence, previous attempt,
policy revision, counters and deadline. Separate transport retry, model-quality
escalation and task decomposition; each has its own bounded counter but all share
the root cap. Increment the admitted action's counter durably so restarting cannot
reset its allowance. Backoff occurs outside store transactions and stops on pause,
new steering, authority loss or deadline.

The handoff packet contains objective, constraints, active instructions and source
refs, current diff/base/file identities, accepted decisions, findings with evidence,
outstanding questions, checks and their tested fingerprints, unresolved tools and
charges, authority scope and remaining allocation. Full source artifacts remain
available for authorized inspection; the next prompt is a bounded projection.
Do not transfer hidden reasoning or invent unobserved rationale.

Before the new request, close or explicitly record every pending tool/result pair.
A smaller context model receives a newly assembled manifest; it does not reuse an
oversized serialized request. Unsupported provider-specific blocks can be omitted
only with an attribution/conversion record and without losing required semantics.
An old stream that finishes after escalation remains evidence for its attempt. Its
output cannot execute tools against the newer step or settle another reservation.
Unresolved prior spend and effects survive the handoff.

## Optimization transactions and evaluation

Proposed `OptimizationReport` fields are workspace, authorized history selector,
canonical cutoff, retention/coverage summary, cohort definitions, included and
excluded task IDs, outcome/cost certainty, grouped metrics and evidence refs.
Construct cohorts by task class, size/complexity signals, fixture or project family,
catalog/provider and policy revisions. Include failures, abandonment and unknown
costs; display denominators. Historical access is checked at query time. A captured
report becomes unavailable or redacted when its referenced content loses access.

`OptimizationProposal` binds report ID, base policy revision, prior preference
answers, selected field changes, per-change rationale, uncertainty and optional
trial plan. Distinguish user preferences from statistical findings. Local summaries
and interview logic can create a proposal without a model call. Optional remote
analysis receives only authorized report/context artifacts and an admitted
optimization task; it cannot execute its returned suggestions.

Validate selected fields against a closed editable-field schema, reject unsupported
fields and calculate effective values under trusted ceilings. Preview both the
stored delta and resulting effective policy so an overridden preference is clear.
Apply by compare-and-swap against the base revision: persist new revision, selection
receipt and event together before acknowledging. A concurrent change requires a
rebased preview; it is not merged silently. Rollback creates a new revision whose
selected values derive from the chosen predecessor, preserving the audit trail.
Revalidate that restored values remain legal under current ceilings and catalog.

Budget raises, authority changes and pruning are distinct commands requiring their
own explicit user decisions; a routing diff cannot smuggle them through as ancillary
fields. A selected preference does not authorize paid comparison trials. Interrupted
apply either has a durable result reachable by command ID or has not committed.

P6-04 evaluations declare task pools, training/tuning versus held-out partitions,
policy candidates, budgets, intervention rules, retry limits and grading rubrics
before running. Compare strategies on matched inputs and count unsuccessful attempts
and all support cost. Report distribution and uncertainty, not just averages or cost
per successful task. A quality floor remains a gate at every profile. Later P8 runs
must include real child/handoff behavior before defaults are called release-qualified.

## Skill discovery and activation

A proposed `SkillDescriptor` contains stable package/component ID, version,
description, activation cues, environment/tool requirements, relative body/resource
paths, body/content hashes, license/source and supported VCP version range. The
descriptor is data. Validate bounds and path syntax before resolving resource
paths relative to the selected package root; then enforce canonical containment
and link/reparse-point policy from repository discovery.

Use an explicit source registry of built-in, user-configured and workspace sources.
Trust does not follow the shortest path or newest modification time. Resolve
duplicate component IDs with a recorded source-precedence policy; report ambiguous
same-precedence duplicates instead of loading an arbitrary file. A user-selected
qualified component identity avoids shadowing. Cache descriptor scans by source
revision with bounded file count/bytes/depth and deterministic ordering. Loading
descriptors must not run scripts or load every body.

Activation resolves the exact descriptor and its resources, rechecks hashes and
scope, records user/engine trigger and body refs, then passes content through normal
context precedence. A skill body cannot override explicit instructions, trusted
denials or current grants. Tool requirements are eligibility/setup information,
not permission to install tools. Surface activation and unavailable prerequisites
in CLI events, including why an explicitly requested skill could not load.

When content changes, future contexts use a new descriptor/body revision. Invalidate
prepared operations only when their context or preconditions depend on the changed
content; retain historical activations as evidence. Disabling a component blocks
new activation and causes the controller to assess active work. Already-dispatched
effects still require receipts and reconciliation. Removal cannot erase history or
unsettled dependencies.

The catalog coverage record separates analyze, review, generate and execute-check
support per ecosystem and host/toolchain. Each skill fixture specifies detection
inputs, scoped instructions, expected project commands and prohibited changes,
missing-tool cases, and an output rubric. Grade chosen paths, evidence and behavior;
do not require a particular paragraph of model prose.

## MCP identity and dispatch

Select a supported MCP specification revision and transport subset during P7-03;
record them with conformance fixtures rather than treating a moving upstream SDK as
the contract. Required tool/resource behavior is governed by
[architecture section 15.4](vcp-what.md#154-mcp-lifecycle). Do not enable optional
server-initiated model calls, roots exposure or other capabilities merely because a
library supports them. Any enabled model assistance must use the VCP gateway/ledger;
otherwise negotiate it as unavailable.

`ServerRegistration` proposes a stable local registration ID, configured endpoint
or executable identity, transport/version policy, workspace scopes, auth-reference
IDs, capability allowlist and connection limits. Credentials resolve inside the
adapter, never in serialized configuration, prompt context or receipts. Executable
startup uses the existing process broker; remote connections require configured
network/data authority. Auth expiry/revocation blocks new dispatch and surfaces
setup state without logging secrets.

`McpToolIdentity` binds registration ID, peer identity evidence where available,
connection generation, negotiated protocol version, remote tool name and schema
digest. Display names can collide. Parse discovery into bounded normalized schemas
and preserve the original permitted metadata as attributed artifacts. Do not let
an annotation declare a tool read-only for VCP authority; trusted configuration and
qualified behavior determine its effect class. Unknown effect classes serialize
and use conservative approval/scope rules.

For each call: pin schema/identity, validate complete arguments and limits, create
a prepared operation containing current policy/steering/workspace and resource
preconditions, obtain applicable authority, durably record dispatch intent, recheck
the connection generation and authority, and invoke through the broker. The intent
plus original request identity must survive a crash before a response. Persist
result/error/effect observations before acknowledging completion. Bounded display
content may truncate while the retained observed artifact records the true capture
limit and truncation state.

Arguments containing workspace/history/memory content bind source artifact and
context provenance plus current access, authority and deletion revisions. At the
common dispatch fence, recheck that every referenced source is still eligible for
the configured recipient. Revocation or pruning invalidates prepared uploads even
when their serialized argument bytes are already cached; it cannot be bypassed by
reusing the same tool schema or approval. A stale operation must rebuild from
currently permitted inputs and obtain applicable authority. Add a fixture that
pauses between preparation and dispatch, prunes an input and observes no upload.

Reconnect creates a new connection generation, rediscovers capabilities and checks
schema equality. Prepared calls bind the old generation and require revalidation;
schema or identity changes require fresh preparation/authority. Do not assume a
transport request ID is a durable remote idempotency key. Cancellation and timeout
stop local waiting but cannot prove the remote effect did not happen. Read an
independent remote receipt/status if supported; otherwise keep `outcome_unknown`
and require an explicit reconciliation decision. Never use a repeat write as the
status probe.

Resource and prompt content remains external evidence with server and URI identity.
Access checks apply to every read, including cache hits, and no returned instruction
can grant authority. Avoid automatically dereferencing returned URLs or passing
resource URIs to local file APIs. Connection shutdown stops new calls, cancels what
can be cancelled, records unfinished work and preserves result/charge references.

## Child graph and workspace snapshots

Graph mutation is a controller command. Validate node/dependency existence, same
root, acyclicity, depth, acceptance and allocation before publication. Use durable
task states from [section 4.2](vcp-what.md#42-task-and-turn-semantics); waiting for a
resource is a reason/stage, not an undocumented terminal state. Runnable nodes have
satisfied dependencies, current authority, an accessible base, sufficient allocation
and a live owning root. Apply root/global model, process and write concurrency caps.
A one-call profile may run useful children sequentially.

Child authority is the intersection of current root scope, parent grant, assignment
scope and host enforcement capabilities; never clone a mutable all-powerful grant.
Use the same root ledger and tag attempts with child IDs. An allocation is a ceiling
inside the root cap, not a second settled charge. Cancelling a node does not release
submitted reservations or unresolved liabilities. Narrowed root authority invalidates
queued child dispatch and forces active effects through common cancellation rules.

Proposed `WorkspaceSnapshot` fields: workspace/host identity, base commit if any,
index/staged representation, tracked working-file contents, authorized untracked
contents, path types/permissions relevant to the host, excluded-path inventory and
content fingerprints. Never include ignored or secret files by default merely to
produce a perfect directory copy. Mark omitted required inputs and stop or use a
scoped supported execution path. Preserve staged and working versions separately
so materialization cannot erase the user's original index state.

Capture a consistent snapshot by acquiring the engine's workspace ownership and
verifying observed file/index revisions before and after collection. Human writes
may still race: detect changes and retry a bounded number of times or ask for a
stable boundary; no claim of an atomic filesystem snapshot without a qualified
host primitive. Materialize outside the user's worktree in a distinctly registered
disposable location. Validate the resulting fingerprint before releasing the child
to run. For non-Git directories use the qualified copy strategy or serialized
ownership. Do not commit user changes just to construct a child base.

Write-set declarations support conflict scheduling but are not enforcement. The
broker's actual write scope constrains effects. Overlapping child branches can stay
isolated, but integration serializes on current parent versions. Read-only results
must cite the examined snapshot because subsequent parent edits may stale findings.

## Integration and progress recovery

`ChildResult` binds assignment/base/snapshot identities, observed changed paths,
patch or result artifacts, checks with tested fingerprints, evidence/findings,
unresolved effects, status and known/uncertain charges. The controller computes the
actual child diff and compares it with the packet. A malformed or out-of-scope result
cannot be integrated merely because the child says its tests passed.

Treat integration as a prepared edit with its own intent and receipt. Perform a
three-way comparison between child base, child result and current parent; include
staged and unstaged parent state in conflict analysis. Do not overwrite the user's
index to simplify merging. Disjoint changes can be prepared against the current
parent revision; conflicting changes need resolution followed by new preparation.
Recheck before apply, record per-path outcomes, and preserve partial effects if
interrupted. Compensation is a new revision-checked edit, not unconditional rollback
over later user changes.

After apply, select checks for the actual integrated fingerprint and invalidate
affected prior verification. Parent completion references this evidence and child
ancestry. A failed integration can retain useful findings without asserting the
child's patch was accepted. Cleanup verifies registered absolute worktree paths,
ownership and no remaining recovery/artifact references; cleanup failure is a visible
maintenance issue, not a reason to delete broader directories.

Progress events use the session's durable ordering and carry child/root/parent IDs,
stage, state, causation, model/group, current tool, workspace ref, useful commentary
and ledger-derived cost. Do not accept child-provided totals as authoritative. A
bounded UI can coalesce transient token deltas; lifecycle, input, failure and terminal
events remain recoverable by cursor. Heartbeat records expose last observed activity
and a waiting reason without fabricating progress.

Owner loss first prevents new root/descendant dispatch, then cancels in-flight work
through common brokers and checkpoints graph, worktree and liability refs. Resume
reconciles effects, validates host/worktree/base identities and authority, and only
then makes nodes runnable. A missing worktree produces a visible blocked node with
retained evidence. New steering applies to queued children; obsolete child findings
remain historical and cannot overwrite the new objective.

An explicit `/pause` uses that same dispatch barrier while leaving the CLI open.
Status, `/agents`, costs and authorized read-only inspection remain available while
the tree is paused. A status query or heartbeat does not resume work. `/resume` is
a deliberate command that performs reconciliation and revalidation before root,
child or task-scoped model, extraction and observer work dispatches again. Receipt
reconciliation and the configured local pause checkpoint remain permitted under
their existing authority; they cannot launch new task effects or hidden model work.
Test this connected pause separately from terminal-close recovery; closing the
application is not required to stop work.
Track individual child pause/cancel separately from inherited root pause so root
resume cannot accidentally restart a child the user deliberately stopped.

## Deferred hook import and observer contracts

These contracts belong exclusively to P10, after P8-05; none is needed to load a
built-in skill, call MCP or delegate in the first release.

Hooks receive a versioned event projection with hook/source version, root/task IDs,
causation depth, input identity, scoped artifact refs, timeout and effect scope. Plan
hook order deterministically from configured priority and stable identity; reject
ambiguous or cyclic declared ordering. A hook execution uses the normal broker and
durable receipts. Its validated output may propose context or a rewritten operation,
but the latter gets a new operation identity and fresh policy evaluation. Failure
policy is explicit: security/validation gates block the affected action; optional
notification failures remain visible. Deduplicate by event/hook/input identity;
unknown external outcomes are not rerun on restart.

Configuration import uses an immutable source snapshot, format/version detector,
supported-field mapping and normalized proposal. No imported string is executed
while parsing. Preview includes source path/provenance, each supported mapping,
conflicts, unsupported fields, secret-reference gaps and authority differences.
Apply chosen fields with revision checks and a reversible local configuration
transaction. Credentials are reconfigured by reference, never copied into history.
Hooks remain inert unless their separate trust/authority requirements are met.
Compatibility means a tested source/version/field subset, not a familiar filename.

Observers subscribe to selected durable events and produce controller proposals.
Use a deduplication identity of observer version, root task, trigger class and relevant
manifest/diff revision; debounce before reserving a model call. Persist the selected
input and outstanding attempt so restart cannot review unchanged work repeatedly.
Queue/concurrency/deadline/call caps and root budget admission apply. A stale result
may be shown as historical evidence but cannot mutate a newer task revision. Owner
pause stops observer dispatch. Evaluate disabled/enabled variants on matched work
with total cost and interventions before proposing an enabled default.

## Acceptance evidence and unresolved choices

Use deterministic fixtures under the planned `src/tests/fixtures/` and contracts
under `src/tests/contracts/`; place actual worktree/close/process cases in native
Windows/recovery suites and model-quality tasks under `src/evals/`. Register real
targets when implemented. Assert independent filesystem/remote effect counts and
ledger/event facts, not just adapter return values. Fault barriers include selection
versus admission, intent versus dispatch, remote completion versus local receipt,
snapshot versus child launch, and integration apply versus verification.

P6 owns numerical quality floors, catalog staleness policy and profile defaults;
P7 owns the qualified MCP revision/transports, source precedence and child snapshot
implementation; P10 owns supported hook/import subsets and observer defaults. The
ADR register's routing/extension/delegation decisions must record those choices
with evidence. This design supplies reviewable implementation boundaries while
leaving unmeasured values and unsupported compatibility claims unresolved.
