# 12 — Model groups, cost profiles and project optimization

Status: in progress. The [routing foundation](../evaluations/p6-routing-foundation.md) implements deterministic registry/selection, retained host admission, escalation and local optimizer controls. Live profile/evaluator qualification and the remaining policy surface are still acceptance gates. Owns P6-01 through P6-05. Registry work follows P2-02 and P5-06; final profile qualification also needs P5-08. Architecture sections 7–8 govern routing. [Model research](../architecture/model-groups.md) supplies candidates, not shipping ranks/prices.

## Code organization

The [Markov supplement](21-markov-integration.md) adds planned increments within
these owners. Start M1's shared evidence/analysis foundation under P6-02, then
M2 under P6-03, then M3 reporting under P6-05 and M4 qualification. M7's offline
threshold proposals/provider-burst experiment follow that evidence. These are
unimplemented extensions, not new task dependencies or enabled defaults.

Use `vcp-models/catalog` for gateway metadata. The logical `vcp-routing` registry, eligibility, policy, selector, explanation, escalation and handoff boundaries map into `vcp-models::{routing,escalation}`; canonical optimizer/publication services map into `vcp-lifecycle::foundation::routing_state`. This retains existing dependency boundaries without another gateway or database. CLI interview/policy-diff presentation belongs in `vcp-cli/optimize`; policy revisions and outcome observations use the canonical store.

P6-02 also owns the logical `vcp-decision` contract, deterministic evaluator and
optional thin Rust OpenRouter adapter; P6-03/P6-05 consume its bounded advice.
The [canonical routing shadow increment](../evaluations/p6-decision-shadow.md)
connects actual admitted routes to separately budgeted native/comparator
observations. It preserves the baseline and leaves live qualification, advisory
consumers and qualified fallback transitions incomplete.
Qualify deterministic rules as the baseline, actual Jev
through OpenRouter as the preferred specialized candidate, and a conventional
OpenRouter LLM as a comparator and explicitly permitted fallback. Add M4's local
statistical comparison for supported purposes, with separate provenance and
qualification from native Jev probabilities. Map proposed
`question`, `answer`, `validation`, `evaluation` and adapter modules into the
existing workspace only when implementing useful behavior. This is not a second
gateway, policy authority, scheduler or store. Follow
[ADR-020](../adr/020-bounded-semantic-decisions.md) and the
[decision design](../architecture/decision-evaluation-design.md#decision-contract).
The [Jev exploration](../architecture/exploring-jev.md) motivates the abstraction.
[Jev-through-OpenRouter qualification](../architecture/decision-evaluation-design.md#jev-through-openrouter-qualification)
defines the proposed actual-model integration; no measured default, source import
or direct TypeSafe service integration is implied.

`RoutingInput` binds task/role, capability/context needs, project policy, model pin/fallback permission, catalog/evaluation versions and remaining budget. `RoutingDecision` records all candidates, exclusions, selected group/model/provider, estimated cost, evidence and escalation reason. Final reservation uses the actual assembled request size.

Read the proposed [routing and extension design](../architecture/routing-extensions-design.md#service-boundaries-and-persisted-identities)
alongside [gateway/group ADR-006](../adr/006-model-gateway-and-groups.md),
[routing ADR-007](../adr/007-profiles-and-routing.md) and
[optimization ADR-017](../adr/017-project-optimization.md). These supply interfaces
and decision work, not qualified model defaults. Map the logical modules to the P0
source map before writing code; use existing canonical policy/artifact/ledger
interfaces rather than creating a router-owned database or gateway.

## P6-01 — Versioned registry

1. Model Frontier/High/Medium/Low as capability groups, separately from user profiles low/med/high. Group membership carries evidence date/version and role suitability.
2. Import research candidates only after current model/provider availability and parameter support checks. Preserve user exclusions and pins; unknown price/context/tool support cannot silently become eligible.
3. Store compatibility observations, quality data, usage/latency distributions and source provenance. A catalog refresh creates a new revision without changing an active request.

Test removed/renamed candidate, stale price, unknown capability, contradictory research membership and provider restriction. Persist an inspectable rejection reason for every ineligible candidate.

**Construction sequence.** Define immutable `CatalogRevision`,
`CompatibilityObservation` and `GroupMembership` records using
[catalog identity rules](../architecture/routing-extensions-design.md#catalog-and-routing-decisions).
Keep raw permitted metadata separately from normalized fields; validate exact IDs,
currency/units, nonnegative rates, context/output constraints and capability
tri-state before publishing a revision. Preserve unknown fields in source evidence
where safe without granting them operational meaning. Aliases do not inherit
qualification automatically. Store observation time and any source effective date
separately so freshness is testable without rewriting historical facts.

Implement refresh as fetch/parse/validate/stage followed by a short canonical
revision-pointer transaction. Failed refresh leaves the prior snapshot available
with visible staleness; an active attempt retains its original limits and prices.
Concurrent refreshes use expected revision checks. Compatibility probes are explicit
bounded evaluation work with task ownership and reservations, never paid requests
hidden in a catalog read. A deterministic fake catalog must cover missing fields,
provider-specific exceptions and removal during an active request. Expose the
source/age/qualification state in group inspection rather than only a group label.

Deliver normalized fixture snapshots, a migration/reopen case for retained catalog
revisions and a source-to-field map in the future model-adapter development guide.
Recheck [architecture section 7.2](../architecture/vcp-what.md#72-catalog-and-compatibility-registry)
when choosing what the gateway can actually report; unexposed provider identity or
model version remains an explicit limitation.

Record Jev as a distinct evaluator candidate, using `typesafe/jev-1.13` only as the
current planning example. Refresh its exact OpenRouter model/endpoint identity,
availability, constraints and prices during implementation; record observation
dates and immutable model revisions where exposed. A renamed alias or successor
does not inherit prior qualification. Do not freeze today's price or treat an
OpenRouter catalog entry as proof that VCP's request/response contract works.

## P6-02 — Deterministic profile policy

M1's [task-transition evidence increment](../evaluations/p6-transition-evidence.md)
establishes scoped historical observation and gap handling through a local
inspector. The [pure numerical increment](../evaluations/p6-markov-kernels.md)
adds bounded counting, absorbing-chain solves, rewards and log likelihoods.
The [causal action increment](../evaluations/p6-action-evidence.md) adds separate
turn/effect/attempt traces, exact cohorts, retry lineage, bounded counters and failure
identities. The [charge-reward increment](../evaluations/p6-charge-rewards.md)
adds exact per-attempt settlements, late debit/credit reconciliation, currency,
reserved liability and terminal-cost abstention under
[ADR-031](../adr/031-exact-attempt-charge-attribution.md). Fitted-model integration
continues with a [rebuildable first-order candidate](../evaluations/p6-fit-provenance.md)
whose source, parameters, counts and abstention are explicit under
[ADR-032](../adr/032-rebuildable-markov-fit-artifacts.md), a
[held-out order comparison](../evaluations/p6-heldout-order-comparison.md),
[exact reward mapping](../evaluations/p6-reward-mapping.md) and
[consumed-value replay](../evaluations/p6-consumed-value-replay.md). This completes
the M1 foundation; no current candidate is a qualified routing estimate.

**Markov increments:** [M1](21-markov-integration.md#m1--retained-evidence-and-analysis-foundation)
is implemented before the next P6-03 advisory integration: bounded causal history projection,
closed state/action alphabet, cost attribution, numerical routines and derived
artifact retention/rebuild. After [M4 qualification](21-markov-integration.md#m4--qualification-and-routing-estimates),
refactor configured `CostEstimate` components to accept qualified evidence with
explicit fallback, provenance and checked upward rounding. Expected path cost
must remain separate from actual request bounds and atomic reservations.

Implement an explainable selection pipeline: required capability/scope/provider filtering, measured quality floor, total estimated task cost and profile-dependent latency/capability preference. Low emphasizes fast inexpensive work with bounded escalation; high prioritizes capable successful completion under the same hard spending policy. Profiles need not map to one group.

Include context transfer, retries, support roles and reserved verification in estimates. Select inexpensive eligible support roles even when the main model is stronger. Use deterministic tie-breaking from recorded inputs; keep unexplained learned routing deferred until sufficient evidence exists.

The deterministic baseline remains the default. A separately enabled, qualified
semantic evaluator can provide bounded Boolean, Choice or Score advice among
already eligible alternatives. The final selector remains a deterministic
function of recorded inputs, including any validated advice. Replaying a recorded
decision must not make another model request; reproducing a live provider answer
is not promised. Disabled evaluation always uses the recorded baseline or existing
visible stop/input condition and makes no evaluator call. Unavailable, malformed,
stale or abstaining evaluation follows that same path unless the enabled policy
explicitly permits a separately qualified conventional OpenRouter evaluator
fallback after current-input revalidation and under the same remaining limits.

Tests cover a cheap candidate that fails the quality floor, a fast candidate with expensive retry history, insufficient verification reserve, a strict pin and concurrent root allocations. Compare decisions against explicit eligibility/ordering assertions, not a duplicated implementation function.

**Construction sequence.** Implement the selector as a pure function over versioned
inputs, emitting ordered candidate records with stable exclusion reason codes.
Separate role suitability from task profile and separate measured expected total
cost from the immediate reservation maximum. Use exact monetary arithmetic from
P1-05; unknown charge components cannot be rounded down to zero. The quality floor
and comparison order are policy values; P6-04 supplies their measured defaults.
Record fallback-to-broader-cohort assumptions when the exact task class lacks data.

Follow the [seven-stage selection pipeline](../architecture/routing-extensions-design.md#catalog-and-routing-decisions).
After selecting a candidate, assemble its actual context/tool schema and validate
capacity before final cost admission. Admission checks current root/child limits
and protected verification reserves atomically. If another child consumes remaining
capacity first, produce a revised decision or a visible budget stop; never dispatch
against the earlier balance. Persist the selected decision and manifest with the
attempt/reservation linkage before calling the gateway. Apply policy changes only
at a revalidated scheduling boundary.

Build table-driven eligibility cases whose expected reasons and winner are stated
independently of production ranking. Add concurrent admission tests with barriers
after selection and before reservation, and assert actual admitted attempts plus
root totals. Inspector fixtures must reconstruct the comparison from recorded
catalog/policy/evaluation inputs, including unavailable candidates and the final
tie break. Preserve [architecture section 7.5](../architecture/vcp-what.md#75-routing-algorithm-and-explanation)
and [root accounting](../architecture/vcp-what.md#81-root-ledger-and-reservations).

**Bounded decision construction.** Build the typed contract and deterministic
implementation together with a real routing consumer and fixtures. Bind each
request to workspace/root/task/step, input and steering revisions, purpose,
question/schema version, permitted evidence references, policy/catalog revisions,
closed option IDs or finite score bounds, deadline and attempt limit. A response
records answer or explicit abstention per required question, evaluator identity,
schema/prompt/configuration versions, permitted evidence references and attempt
attribution. Keep score, model-reported probability and independently measured
calibration distinct; a score is never automatically a probability or confidence.

Qualify the actual Jev endpoint and question/answer protocol before coding a
transport mapping. Determine the documented request mode, authentication through
OpenRouter, supported parameters, question limits, schema guarantees, cancellation,
errors, usage categories and exposed native probability fields. Do not assume a
Chat Completions request with a replaced model name implements Jev semantics.
Use a thin Rust mapping through the shared gateway; no LangChain, Python/JavaScript
runtime, Jev SDK or direct TypeSafe credential is required. Record unsupported or
unobservable features and disable affected question modes until qualified.

Preserve native answer semantics in versioned fixtures: Boolean yes-probability
means the probability of the stated proposition, not an implicit true/false value
obtained with a hardcoded threshold. Rust applies a separately qualified purpose
threshold or abstains. For Choice and Score, preserve the answer, any native
distribution and any provider confidence field separately; confidence is not
automatically the winning choice's probability, the score itself or empirical
calibration. Missing native probabilities remain unavailable, never invented from
free-form text or replaced by one-hot certainty. A qualified discrete-only mode is
a distinct capability and evaluation cohort.

Validate the whole response before use: reject unknown, duplicate or missing
question IDs, unlisted choice IDs, wrong types, non-finite/out-of-range numbers,
invalid distributions and evidence outside the supplied scope. Do not coerce a
missing answer to false, normalize malformed output into confidence or silently
accept a partial batch. Bounds on question count, payload bytes, output and
latency apply before dispatch. Invalid output may receive only a separately
admitted bounded retry; otherwise record abstention and use the baseline. Schema
validity establishes shape, not factual truth or authority.

Implement the optional adapter through existing context assembly, OpenRouter,
capability admission, root accounting and canonical attempt history as specified by
[dispatch and recovery](../architecture/decision-evaluation-design.md#dispatch-and-recovery).
Select its model deterministically from an explicit eligible evaluator policy;
evaluator selection, schema repair and grading cannot recursively call the
evaluator. Disable tools in evaluator requests. Every attempt, repair and shadow
request reserves its full maximum cost before send and settles observed or unknown
usage through the same ledger. Revalidate current revision, scope, pause and
authority before dispatch and before consuming the answer; stale late output is
retained for accounting without influencing current work. Persist prepared input,
attempt/reservation linkage and outcome without holding a transaction across I/O.
Reopening cannot blindly resend an uncertain prior request or clear its liability.

Implement fallback as an explicit policy transition with a reason and source/
destination evaluator identities. A conventional OpenRouter LLM is eligible only
after its question modes, data restrictions, usage and per-purpose quality are
separately qualified. It receives a fresh bounded reservation after current scope,
pause and eligibility checks, while Jev's failed or unknown charge remains charged
or reserved. Shared deadline, retry/call limits, root balance and verification
reserve apply across both implementations. No qualified permitted fallback means
deterministic behavior or the existing visible stop/input state; do not bypass
OpenRouter by calling TypeSafe directly or silently widen provider permissions.
If an old answer is stale, reconstruct the request from current authorized evidence
and revisions before reconsidering fallback. Pause, revocation or task supersession
cannot trigger a replacement call or implicitly resume work.

Initially expose remote evaluation as explicitly enabled shadow comparison:
record its advice alongside the baseline without changing the selected action.
Shadow mode still sends authorized context and spends money, so it requires the
same configured cap and visible attribution. P6-04 decides whether any individual
purpose can graduate to advisory use. No direct TypeSafe endpoint/credential, Jev
SDK or local inference asset becomes a requirement. Earlier P2/P5 work must remain
usable without this later P6 service.

Test deterministic-only and disabled modes with a transport that fails on any
request. Use adversarial schema fixtures, evidence revocation, zero budget,
concurrent child admission, pause before send, late response after new steering,
timeout with uncertain charge, restart and retry exhaustion. Assert eligible
candidate membership and quality floors after advice, no recursive helper calls,
all observed requests joined to reservations, and continued baseline behavior.
Add actual-Jev protocol fixtures for missing probability fields, unsupported modes,
outage, alias/version drift and changed provider data controls; verify fallback is
qualified, permitted and separately attributed. Include adversarial state strings
that imitate question definitions or inject options. Exact counts, date comparisons,
arithmetic and authority/scope checks run in Rust, with fixtures proving remote
judgment cannot override their results.

## P6-03 — Escalation and model handoff

**Planned Markov increment:** [M2](21-markov-integration.md#m2--escalation-integration-and-local-stall-comparison)
first completes canonical advisory persistence/scheduling/revalidation, then adds
exact-cycle evidence and a local statistical shadow producer. Reuse M1's symbols
and revisions; do not add another event log or remote-only contract. Qualify
transition influence in P6-04. The [M7 provider-burst experiment](21-markov-integration.md#m7--offline-proposals-and-provider-bursts)
comes later and preserves transport/quality counter separation and strict pins.

Define bounded triggers from invalid tool output, repeated failed verification, unsupported capability or declared task complexity. Record trigger/evidence and remaining budget. Reassemble for the new capability envelope and preserve objective, current changes, applicable instructions, unknown effects and tool/result pairing.

No escalation widens autonomy or provider-data permissions. An ambiguous request remains accounted for while a later attempt uses its own reservation. Repeated failure reaches a visible blocked/failed/input state rather than cycling indefinitely.

Test smaller-context fallback, tool-schema change, repeated failures, paused escalation and late response from the prior model. E04/E11/E12/U07 apply.

**Construction sequence.** Add a typed `EscalationTrigger` with evidence refs and
separate retry/quality/decomposition counters. Persist counters with admitted
attempts so recovery cannot reset them. Store deadline and backoff state outside
the model prompt; a request cannot increase its own remaining tries. Treat strict
pin failure as a blocked/input condition unless recorded fallback permission
allows the candidate. An escalation changes routing, never grants or data scope.

Build `HandoffPacket` from canonical current state using
[handoff barriers](../architecture/routing-extensions-design.md#handoff-and-escalation-barriers)
and [architecture section 7.6](../architecture/vcp-what.md#76-handoffs-and-errors).
Include unresolved effects, exact diff/file versions, applicable instructions,
remaining liabilities and acceptance checks. Rebuild tool schemas/messages for the
new model; validate pairing before admitting spend. Persist conversion/omission
reasons for provider-specific fields and preserve original artifacts. Stop an old
stream from issuing tools after its step is superseded, while still recording late
usage and observed output against that old attempt.

Inject cancellation/new steering between trigger, handoff assembly and admission.
Assert no new dispatch while paused, no lost constraint on a smaller-context model,
bounded total attempts after restart and separate settlement of late prior usage.
Use an independent marker tool to prove fallback did not repeat a previous remote
effect merely because its result was absent from the new model context.

Add bounded advisory purposes for repeated-strategy suspicion, retry/replan/
escalate/stop preference and independent-review triage using the
[consumer boundaries](../architecture/decision-evaluation-design.md#consumer-boundaries).
The [typed escalation advisory contract](../evaluations/p6-escalation-advisory.md)
now binds these closed questions to source revisions and preserves every existing
deterministic gate. Its finite qualification capability,
[canonical request/result record seam](../evaluations/p6-advisory-records.md) and
[caller-owned scheduling lease](../evaluations/p6-advisory-scheduling.md) are
implemented. The [helper accounting binding](../evaluations/p6-advisory-accounting.md)
reuses the canonical budget ledger. The
[canonical shadow runtime](../evaluations/p6-advisory-runtime.md) connects the
caller-owned transport and response path for admitted invalid-output and
failed-verification escalations. Unsupported trigger evidence mappings abstain;
live qualification remains open.
Supply bounded observed actions, errors, diffs and checks with source revisions;
do not invent access to full provider reasoning. Rust evaluates fixed trigger,
attempt, budget and authority constraints before and after advice. A low risk
score cannot suppress required tests/review, override a hard failure or mark a task
complete. Abstention and provider failure preserve required review and existing
bounded escalation. Optional additional review is still an admitted task.

Test misleading advice to continue a capped loop, skip a required review, approve
an unverified diff or escalate outside the permitted candidate set. Include true
stalls, productive repeated attempts and seeded serious defects, recording missed
as well as false triggers. P8 repeats relevant cases with actual children; P6's
scripted child events are preliminary evidence only. Continuous background
observer agents remain P10-03 deferred scope.

## P6-05 — `/optimize` workflow

**Planned Markov increments:** after the P6-03 implementation prerequisite,
[M3](21-markov-integration.md#m3--read-only-forecasts-in-the-optimizer) adds read-only
transition/reward forecasts and drift reports using M1's shared foundation.
Refactor existing current-state counts without pretending they reconstruct past
transitions. [M7](21-markov-integration.md#m7--offline-proposals-and-provider-bursts)
then adds offline threshold hypotheses through preview, selected apply and rollback
after M4 evidence; it does not infer unobserved actions or authorize paid trials.

1. Read a scoped history window and calculate task classes, languages, failures/abandonment, retries, intervention, token/effort, cost certainty, latency, child overhead and retrieval contribution. Mark missing/pruned/small samples.
2. Build a baseline report with evidence links and uncertainty. Avoid presenting correlations from unlike tasks/providers as causal improvement.
3. Ask a small adaptive interview about spend/speed/quality, expected task size, review preferences and model restrictions. Reuse current project answers; no complete questionnaire on every invocation.
4. Produce a versioned policy diff for groups/roles, effort/output, context/retrieval, escalation and concurrency. Explain intended effect and evidence. Keep pruning suggestions separate from executable deletion.
5. Apply only the developer's selected changes within trusted ceilings, retain the prior revision and implement rollback. No silent budget increase, grant change or extra paid trials.
6. Optional model assistance uses a separate admitted/attributed optimization task; local analysis and questions remain available without it. Observe later tasks and report regression without claiming retraining.

Tests: empty history, only failed tasks, pruned comparison period, unknown charges, an attempted budget/grant escalation in a model suggestion, declined changes, interrupted apply and repeated rollback. Confirm exact old/new effective policy revisions and no deletion or unrequested model call.

**Construction sequence.** Add read-only aggregation over authorized task/history
services with an explicit canonical cutoff and cohort key. Produce counts and
denominators before rates: include completed, failed, cancelled and abandoned tasks,
split known and uncertain spend, and mark missing/pruned evidence. Keep task-class,
provider, policy and size differences visible; do not mix unrelated cohorts into an
asserted routing improvement. Persist a report with permitted evidence references,
not copies of inaccessible history. Recheck access when displaying saved reports.

Implement the interview as durable question/answer state tied to existing project
preferences and report gaps. Choose the next question for an unresolved decision;
skipping optional questions still permits a user-selected preference update. Derive
a closed-schema proposal containing base policy, field-level old/new values,
reasons, uncertainty and effective values under trusted ceilings. Optional model
suggestions are untrusted input to the same validator. Keep the local workflow
usable when the model budget is zero.

Use [optimization transactions](../architecture/routing-extensions-design.md#optimization-transactions-and-evaluation)
to implement preview, selective apply and rollback. Commit a new revision plus
selection receipt/event with expected-base compare-and-swap; no network call or
question wait belongs inside that transaction. A concurrent policy update requires
a refreshed preview. Rollback writes another revision and checks present ceilings,
rather than deleting history or restoring obsolete authority. Subsequent regression
reports compare declared cohorts and can recommend rollback without applying it.

Add interruption barriers immediately before and after policy publication, duplicate
apply with the same command identity, rejected unsupported proposal fields and a
rollback under newly narrowed trusted limits. Record persisted versus effective
policy in the fixture result. Acceptance follows
[architecture section 7.8](../architecture/vcp-what.md#78-interactive-project-optimization),
including no hidden trials, pruning or budget changes.

Keep deterministic aggregation separate from optional decision-assisted waste
classification and policy suggestions. Bind any advice to the report cutoff,
cohort, question version and current access; distinguish an observed count from
the evaluator's hypothesis about its cause. Include evaluation/repair/shadow cost
and unresolved usage in the report, so a purported saving cannot hide the cost of
its own measurement. Show evaluator enablement and per-purpose mode/limits in an
explicit policy preview; accepting an unrelated routing change cannot enable paid
evaluation or a new provider. Declining advice, unavailable evaluation and rollback
must leave the local interview and selected policy workflow usable. Test a
misleading saving estimate and a proposal to weaken a quality or authority gate.
Separate actual Jev, conventional-LLM fallback and deterministic observations in
cohorts and explanations. A fallback's result or cost cannot be presented as Jev
performance, and a new alias or missing native output field invalidates affected
comparisons rather than silently inheriting an earlier recommendation.

## P6-04 — Profile qualification

**Planned Markov increment:** [M4](21-markov-integration.md#m4--qualification-and-routing-estimates)
adds local statistical advice as a fourth comparison arm, forecast/interval
validation and per-purpose activation/rollback gates. Freeze selected context and
verification refactors before final comparison; later changes invalidate affected
evidence. No probability bypasses deterministic quality or completion gates.

After P5-08 memory evidence is available, compare fixed economical, fixed stronger and routed strategies on the same versioned analysis/review/generation pool. Record all attempts and supporting costs, held-out task outcomes, wall-time distributions, interventions and quality failures. Pin catalog/provider/configuration versions for each run.

Select thresholds and defaults from declared evidence and product priorities, not the research workbook's ranking or an illustrative dollar amount. Initial subsystem tests can script child events/accounting; P8 must recheck total costs and quality with the actual delegation implementation before release.

**Construction sequence.** Create versioned evaluation manifests under the planned
`src/evals/tasks/` and synthetic held-out inputs under `src/evals/fixtures/`.
Declare tuning/held-out partitions, fixed-strategy baselines, profile candidates,
caps, retry limits, allowed human interventions, grading rubric and exclusion rules
before running. Attribute every main/helper/retry/compaction/optimization charge,
and report unresolved charges as uncertainty rather than dropping the run. Missing
provider availability yields a not-run comparison with reason, not a substituted
unrecorded model.

Run identical fixture revisions and controlled starting state for each strategy;
record execution order and catalog/provider drift where live services cannot be
frozen. Compare quality-floor violations, success, total spend, latency distribution
and interventions together. Cost per success alone can conceal costly failed tasks.
Use deterministic fixtures for decision/recovery correctness; live evaluations
require explicit configured spend limits and cannot be ordinary unit checks.
Retain raw evidence in ignored artifacts and publish only reviewed synthetic or
redacted summaries. Use
[the evaluation design](../architecture/routing-extensions-design.md#optimization-transactions-and-evaluation)
and [release acceptance ADR-018](../adr/018-release-acceptance.md).

Deliver proposed defaults with evidence cohort, uncertainty, catalog freshness and
rollback conditions. If no policy meets the quality floor under its configured
budget, report that result instead of weakening the floor. P8 repeats relevant
comparisons with actual child isolation/integration and packaged CLI behavior;
subsystem qualification must identify that pending release evidence explicitly.

**Decision-layer qualification.** Follow
[qualification and rollout](../architecture/decision-evaluation-design.md#qualification-and-rollout)
within the same coherent P6 milestone. Compare four implementations on the same
supported purposes: deterministic rules, local statistical signals, actual Jev
through OpenRouter, and a conventional OpenRouter LLM comparator/permitted
fallback. A stronger conventional LLM is an optional additional comparison, not
a substitute for testing actual Jev. Complete the
Jev protocol/capability/usage qualification first; unavailable actual-model access
is a not-run comparison, not evidence from an emulation. Control the downstream
coding strategy and inputs. First run shadow mode, then evaluate declared
per-purpose advisory policies on a held-out set; shadow agreement alone cannot
establish a task-level improvement. Declare tuning, calibration and held-out partitions,
task/cohort sample requirements and severity-specific false-negative limits before
running. Keep raw scores distinct from probabilities; measure calibration or Brier
score only for outputs defined and evaluated as probabilities, and bind any fitted
calibration to the exact evaluator/prompt/purpose and cohort revision.

Report answer validity, abstention/coverage, false and missed escalation/review,
quality-floor violations, end-to-end outcomes, interventions, p50/p95 latency and
all evaluator/main/helper/repair/shadow/failed-attempt cost. Include resource and
latency overhead when the evaluator abstains or never changes an action. A cheap
classifier that misses serious defects or increases total failed-task spend cannot
pass on average answer accuracy. Lack of sufficient evidence leaves that purpose
disabled; release can qualify the deterministic baseline without a remote
evaluator. Publish enable/disable criteria and rollback triggers per purpose,
including provider/prompt/schema drift. P8 must repeat enabled-purpose comparisons
with real delegation, final integration checks and packaged CLI pause/recovery;
no additional product task or deferred observer dependency is introduced.

Run `routing`, `provider`, E19/U07 and budget/handoff contracts. Done when every decision is explainable/reproducible, optimizer changes are reversible and explicitly chosen, and the proposed profile defaults have a measured quality/cost/latency basis.
