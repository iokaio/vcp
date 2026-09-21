# 21 — Markov analysis and integration sequence

Status: M1–M3 construction implemented; M4 qualification next, September 20, 2026. This execution supplement adopts the useful
directions from [the research note](../research/markov.md) into existing work.
The M1 foundation and M2 frozen local shadow path are implemented but unqualified;
M3 provides read-only action forecasts and saved diagnostics. M4–M10 remain unimplemented or
unqualified. This creates no new product
task IDs, changes no architecture dependencies and enables no runtime feature.
The [owning task sections](20-traceability.md#work-item-ownership-and-readiness)
remain the implementation entry points; the increments below define their added
work, refactors, order and acceptance evidence. Historical P2/P5 acceptance remains
valid for its original scope; it does not cover these planned extensions.

## Scope and dependency order

Prioritize explainable local counts, task-cost forecasts and deterministic cycle
evidence before fitting hidden states or proposing new policies. Qualify graph
context and verification ordering independently. Keep every probability advisory:
it cannot grant authority, release a reservation, resolve an unknown effect,
suppress a required check or prove completion. Rules-only operation remains a
supported configuration, including on an empty or pruned history.

The research suggests `/optimize` first, but P6-05 depends on P6-03. Build the
shared history/numerical foundation under P6-02 first, then the P6-03 consumer,
then P6-05's read-only report. This avoids making P6-03 depend on a completed
P6-05 or making earlier P2/P5 work depend on P6 qualification. Construction and
qualification are distinct: P6-03 implements safe disabled/shadow behavior before
P6-04 decides whether a signal may affect an action. A failed optional candidate
can be recorded as rejected without blocking rules-only release acceptance.

Increment labels below are checklist identifiers within existing tasks, not new
architecture work items. Track their current state in the follow-up ledger. Complete each with linked code,
tests and evaluation results; do not infer completion from this document.

| Order / increment | Owning work | Prerequisites and placement | Deliverable and gate |
|---|---|---|---|
| M1 — Retained evidence and analysis foundation | P6-02; P1-06/P5-07/P5-10 supporting refactors | Next P6 foundation work, before more advisory integration; use existing canonical capture/storage and retention | Versioned transition/action projection, reward accounting, pure bounded numerical routines, rebuild/replay/privacy tests |
| M2 — Escalation integration and local stall comparison | P6-03 | M1, canonical advisory runtime, exact repetition evidence and frozen local shadow runtime implemented; qualification remains M4 | Exact cycle evidence and bounded local shadow producer alongside remote advice; no unqualified transition influence |
| M3 — Read-only forecasts in `/optimize` | P6-05 | M1 and P6-03 implementation prerequisite, including M2's safe baseline/shadow paths | Inspectable forecasts, loops, uncertainty and drift reports; no dispatch or automatic policy change |
| M4 — Qualification and routing estimates | P6-04, P6-02/03 | M2/M3, P5-08 and existing P6-04 prerequisites | Frozen four-arm comparison, calibrated forecasts, opt-in qualified estimate/advice consumption or recorded rejection |
| M5 — Context and retrieval follow-up | P2-01/08, P5-06/08 | Existing completed context/retrieval baseline; independent of M2–M4; compaction diagnostics reuse M1 | Bounded graph/co-change candidates, context comparison, compaction regression analysis; optional memory-graph arm |
| M6 — Verification ordering follow-up | P2-06 | Existing verification baseline; reuse M1 observations before fitting history-based ordering | Dependency-respecting fail-fast order, current-fingerprint checks and unchanged completion gates |
| M7 — Offline policy proposals and provider bursts | P6-05/03, qualified by P6-04 | M3/M4; perform last among routing enhancements | Supported-action threshold proposals, selected policy diffs and bounded trials; burst model evaluated only with enough endpoint evidence |
| M8 — Delegation reuse | P7-04/05/06 | Existing P7 dependencies; reuse M4-qualified forecasts when available | Child/integration cost estimates and visible uncertainty under atomic root admission; baseline works without forecasts |
| M9 — Synthetic campaigns and release recheck | Test segment 16, P8-01/02/05 | Start invented-matrix fixtures with M1; final run after enabled consumers and actual P7 integration | Seeded legal/illegal traces, independent oracles, both stores and packaged Windows qualification |
| M10 — Optional regime observer | P10-03 | After P8-05 and P7-06, as already required; reuse qualified M2 output | Bounded local observer proposals with pause, cursor and revision controls; remains deferred |

M5 and M6 can proceed while P6 evidence accumulates. Finish their selected changes
and refresh affected P5-08 comparisons before freezing M4's final context/config
baseline; a later context or check-policy change invalidates affected comparisons.
M7 trials must not delay a valid rules-only configuration indefinitely. At P6/P8
exit, publish each candidate's implemented, enabled, rejected or explicitly
deferred status and reason. No paid evaluation or trial budget is created here.

## Existing implementation and required refactors

The baseline is merged `main` at `d7e16b1` (PRs #77–#79 include routing shadow
integration, the typed escalation advisory contract and its finite qualification
capability). Canonical escalation advisory persistence/scheduling and live
qualification remain open. Local unmerged work is not implementation evidence.

| Existing boundary | Required change | Preserve / migration evidence |
|---|---|---|
| `vcp-lifecycle::foundation::routing_state::{report, compare_reports}` and `OptimizationReport` | Extract bounded authorized history observation and add a separate versioned transition projection; add forecast/report references and uncertainty | Existing count reports remain readable. Their current-state snapshot semantics must not be relabelled as historical trajectories |
| `vcp-models::{routing, escalation, decision}` | Add pure counting/solving/filtering routines, evidence-backed estimate provenance and a typed local statistical signal interpretation | Separate expected total cost from immediate request bounds; do not pretend local output is native Jev probability or synthesize provider receipts |
| `foundation::worker::{routing, escalation, decision}` and `foundation::decision` | Complete durable advisory lifecycle; share bounded observed symbols, revision binding and post-advice validation across local/remote producers | Root counters, strict pins, quality floors, current authority and independent unknown liabilities still control admission; owner control remains responsive |
| `vcp-context::{selection, manifest, compaction}` and repository discovery | Add bounded optional candidate ranking with source/graph revisions, selected/excluded reasons and navigation trust class; add matched compaction diagnostics | Mandatory context, instruction applicability, tool pairs, exact captured request and send fences remain intact; `ContextSource` is a design term, not an existing API to assume |
| `foundation::verification` and `worker::verification` | Separate applicable check discovery/dependencies from eligible execution ordering; retain check identity, cost/latency observations and ordering provenance | Completion still requires current applicable evidence. Old plans/records without ordering metadata use deterministic baseline order |
| P5-06 query/fusion and generation publication | Evaluate an optional graph candidate stream under current canonical eligibility and versioned fusion | No hidden bridge through revoked claims or stale edges; existing lexical/vector behavior remains available when graph evidence is absent |
| Retention, inspectors and encrypted restore | Register source dependencies for counts, fitted artifacts, reports and consumed signals; invalidate/rebuild derived state after deletion/access change/restore | No aggregate secretly survives source pruning. Historical receipts obey existing retention/redaction rules, and missing evidence cannot reactivate old advice |
| `vcp-cli::optimize`, routing/decision inspectors and later `/agents` | Show counts, cohort, source window, model revision, uncertainty, abstention and selected rule changes | Local analysis makes no network call; a probability or hidden regime is explicitly an estimate, not observed private reasoning |

Version new persisted schemas explicitly. Readers must accept supported older
records through a tested migration or absent-feature path; unknown schemas fail
visibly. Rebuild derived models instead of rewriting prior routing decisions or
requiring them to acquire invented evidence. Before introducing a new artifact or
consumer type in M1/M2, record the detailed boundary and retention choices in a new
ADR, referencing ADR-007/017/020 without rewriting their historical decisions.

## M1 — Retained evidence and analysis foundation

**In progress:** the [first transition-evidence increment](../evaluations/p6-transition-evidence.md)
adds scoped rebuild-on-read task-state observations and explicit gaps through
`vcp optimize transitions`, under [ADR-029](../adr/029-retained-transition-evidence.md).
This independently reviewable increment establishes historical source correctness
before fitting models. The [pure numerical increment](../evaluations/p6-markov-kernels.md)
implements bounded segment counts, observed-support smoothing, absorbing-chain
visits/outcomes/rewards and log likelihoods, with independent synthetic oracles.
The [causal action increment](../evaluations/p6-action-evidence.md) adds separate
turn/effect/attempt traces, retry lineage, exact endpoint/model cohorts, available task
class, bounded counters and revision-bound failure signatures under
[ADR-030](../adr/030-causal-action-observations.md). These increments do not
complete M1. The [exact charge-reward increment](../evaluations/p6-charge-rewards.md)
joins retained settlements once to their owning attempts, preserves late debit/credit
adjustments and currency, excludes rollups, and withholds a point estimate for
unknown or reserved liability under
[ADR-031](../adr/031-exact-attempt-charge-attribution.md). The
[fit-provenance increment](../evaluations/p6-fit-provenance.md) adds a rebuildable,
source-bound first-order candidate with explicit parameters, counts and abstention
under [ADR-032](../adr/032-rebuildable-markov-fit-artifacts.md); it is unqualified
and never persisted. The [held-out comparison increment](../evaluations/p6-heldout-order-comparison.md)
adds task-separated first/second-order scores, complexity penalties and a
first-order multi-step frequency check under
[ADR-033](../adr/033-heldout-markov-order-comparison.md). Reward mapping is now
explicit: the [reward-mapping increment](../evaluations/p6-reward-mapping.md)
keeps exact augmented attempt cohorts separate, reports complete terminal cost
samples, and withholds a cohort mean after any unknown charge under
[ADR-034](../adr/034-exact-attempt-reward-mapping.md). Consumed-value retention
and replay now persist only the selected scalar, producer/input identities and
consumer decision under [ADR-035](../adr/035-consumed-statistical-value-replay.md);
historical replay uses that receipt without refitting. This completes M1. Existing
optimizer report schemas and enabled routing behavior are unchanged.

1. Define a closed, revisioned alphabet from canonical task/turn transitions,
   attempt lineage, tool outcomes and verification receipts. Augment state with
   bounded retry/quality/decomposition counters and relevant task class/exact
   endpoint. Keep observed event classes distinct from inferred regimes. Define
   bounded failure signatures from check identity, applicable input revision and
   normalized diagnostics; changed failures and unavailable signatures remain
   distinct from an exact repeated failure. Do not feed untrusted prose directly
   into action labels or permit text to set policy.
2. Build ordered traces at an explicit canonical watermark and time window using
   causal IDs, not timestamp adjacency alone. Do not join sibling tasks, helper
   attempts or concurrent tools into fictional sequential transitions. Deduplicate
   receipts, retain root/child attribution and specify deterministic ordering for
   independent events. When retained history cannot reconstruct a transition,
   mark the gap/censoring and abstain for the affected calculation. Test whether
   existing events contain the needed facts; add only missing typed observations
   under their owning boundary rather than a second event log.
3. Use completed, failed and cancelled as terminal outcomes where canonical task
   semantics say so. **Blocked is resumable**: represent it as a transient/waiting
   state or an explicitly labelled finite-episode endpoint, never relabel a task
   terminal. Paused, open, abandoned-without-evidence and window-truncated work
   remain censored/unknown. A missing task class, language, intervention or size
   stays unavailable; any additional capture needs a defined source and version.
4. Join each settled charge once to its owning attempt and documented reward
   component. Keep currency, support/child/verification costs, retries and late
   usage attributable without double counting parent rollups. Unknown charges and
   reserved liabilities are not zero-cost visits; exclude an invalid point
   estimate or publish justified bounds with an explicit unknown remainder.
5. Implement small pure Rust routines beside routing/escalation, without a new
   crate, numerical dependency, provider or store. Count rows, smooth only legal
   transitions and solve absorbing-chain expected visits/outcome probabilities/
   rewards with pivot and residual checks. Bound alphabet size, trace length and
   work; use stable ordered iteration and explicit convergence limits. Reject
   non-finite, negative, non-normalized, singular or nonabsorbing models. Never
   invent a path to success through smoothing. Use log-space sequence likelihoods.
6. Version artifacts with source IDs/digest, retained window/cutoff, authority and
   deletion revisions, alphabet/features, task/endpoint cohorts, policy/catalog,
   algorithm/prior/parameters, counts, uncertainty method and qualification ID.
   Sparse rows abstain under declared sample gates; broader-cohort substitution
   must be explicit and qualified. Invalidate serving use after material identity,
   policy, alphabet, access or deletion changes, retaining lawful historical
   interpretation separately from permission to use the model again.
7. Make projections rebuildable from currently retained authorized records on both
   stores. Cached fits are disposable; restore need not transport a new canonical
   model asset. Persist a **consumed** value, input digests, producer version and
   consumer decision so replay uses the recorded value instead of fitting again.
   Source-dependent reports/receipts participate in retention planning: after
   prune/revoke, deny inaccessible content and retain only permitted tombstones or
   metadata. Never resurrect pruned inputs from an old cache or snapshot.

Acceptance: hand-solvable absorbing matrices; exact counter/lineage and cost
fixtures; blocked-then-resumed, partial window, forked tasks, duplicate/late
receipts, missing class, unknown charge, zero/sparse row, singular model, overflow
and non-finite input. Compare first- and second-order fit on held-out traces with
complexity penalties, plus multi-step observed frequencies. Test rebuild parity,
old-schema reopen, retention/restore and access races. Persisted decisions replay
exactly; cross-platform recomputation uses declared numerical tolerances, not an
unsupported bit-identical floating-point promise.

## M2 — Escalation integration and local stall comparison

Finish the current P6-03 gap before adding a second advisory implementation:
canonical request/result persistence, deduplication, caller-owned bounded async
scheduling, pause/cancel handling, reopen and current-input revalidation. A remote
helper retains its ordinary reservation and unknown-charge handling. Local
arithmetic needs CPU/deadline limits and lifecycle ownership, but no fictitious
provider attempt or model charge. Neither path may block the canonical owner from
processing pause/steering. Treat discarded late results as historical evidence.

The first slice is implemented by
[canonical escalation-advisory records](../evaluations/p6-advisory-records.md):
prepared remote inputs and decoded outputs are immutable and exactly deduplicated,
canonical task revisions are revalidated, and changed/deadline-late results receive
a historical-only disposition. The next
[caller-owned scheduling slice](../evaluations/p6-advisory-scheduling.md) adds a
single persistent claim, dispatch revalidation, pause/interruption closure and
reopen without replay. The [accounting binding](../evaluations/p6-advisory-accounting.md)
then reuses ordinary helper reservations and live charge state. The
[canonical shadow runtime](../evaluations/p6-advisory-runtime.md) connects
transport/response handling for admitted invalid-output and failed-verification
escalations. Exact-cycle and local-producer work follow runtime verification.

The runtime implementation and its acceptance checks follow this order:

1. Revalidate completion against current canonical task/workspace revisions and
   current policy/catalog/evidence, including repeated completion calls. Bind the
   helper's retained request evidence and evaluator price identity to the exact
   canonical advisory request; a matching artifact schema alone is insufficient.
2. Capture the admitted escalation plan with the existing verified context seed.
   Derive bounded observations from authorized canonical trigger evidence and
   construct the typed advisory input in the owner. Require an escalation-purpose
   qualification and its exact question revision; routing qualification does not
   authorize escalation evaluation.
3. Run escalation comparisons in host shadow mode through the existing finite
   capability, credential, transport and budget boundaries. Persist a stable run
   identity from the admitted main attempt before dispatch, with purpose committed
   in the request. Only one purpose is installed at a time, and changing it does not
   permit a second comparison of that admission. Regenerating a deadline must not
   permit replay. Close claims and release or retain liabilities
   on every failure between scheduling, reservation, submission and response.
4. Retain sanitized response evidence, decode original bounded bytes and settle
   usage independently of whether advice is current. Recheck source, qualification,
   credential, pause and deadline immediately before the application write and
   before consuming a result. Preserve deterministic escalation and required checks
   throughout this shadow increment; behavioral influence remains P6-04.

Integration fixtures must prove zero calls when disabled or mismatched, one call
per stable trigger, exact request/quote binding, pause and revocation before write,
source changes before consumption, late usage settlement, and interruption/reopen
without replay. These are runtime acceptance conditions, not properties established
by the standalone record APIs alone.

The [exact repetition evidence increment](../evaluations/p6-exact-cycle-evidence.md)
implements bounded verification n-grams and read-only inspection using M1's
symbols and input/failure identities. Other action mappings remain unavailable.
Record observed repetition as a deterministic fact; repeated reading or a
productive repair is not automatically a stall. Add a closed rule only with
explicit thresholds, reset conditions, bounded history and false-trigger tests.
Any new stop/escalate behavior still needs consumer regression and outcome
qualification; the research's “no qualification” observation applies only to
detecting an exact pattern, not to proving the action it should cause.

The [frozen local producer](../evaluations/p6-local-stall-producer.md) supplies
explicit offline fitting and read-only second-order/entropy inference with
separate exact-cycle evidence. The [local shadow runtime](../evaluations/p6-local-shadow-runtime.md)
adds canonical owner selection, asynchronous inference and retained result receipts.
Compare that signal with the deterministic baseline in M4.
Evaluate a small HMM if simpler signals leave material errors; retain it only
with held-out gains. Exploring, converging, thrashing and environment-blocked are
proposed labels. Offline fitting
must be bounded, reproducible from recorded initialization/seed and independent
of held-out labels. Forward filtering is the online computation; optional Viterbi
analysis is retrospective reporting. No continuous online parameter learning.
Reset/invalidate belief at relevant task, steering, workspace, policy or alphabet
changes and make replay/reopen behavior explicit.

Expose the local producer through a typed statistical provenance/validation path
compatible with the bounded advisory consumer, not the remote transport-specific
result type. Initially it answers only repeated-strategy suspicion; it cannot
claim qualified next-action or review triage from the same fit. Define monotonic
composition when multiple producers exist, record disagreement and abstain rather
than silently averaging incompatible probabilities. Closed permitted actions and
Rust checks before/after advice remain authoritative. HMM belief is an estimate;
the same integer threshold convention may be reused only after calibration.

Acceptance: true stalls, productive repetition, flaky/environment failures,
same/new diagnostics, source edits, missing observations, adversarial event
sequences, sparse cohorts, duplicate triggers and old results after steering.
Exercise capped loops, strict pins, retained counters, pause before consumption,
restart, concurrent root allocations and unchanged required review/checks.
Local shadow mode must produce zero gateway requests and cannot alter the chosen
action. Remote qualification remains separate and cannot be inherited by local
signals, or vice versa.

## M3 — Read-only forecasts in the optimizer

**Implemented construction:** [bounded action forecasts](../evaluations/p6-action-forecasts.md)
provide historical episode/cohort reconstruction, observed-support transition
counts, expected visits/outcomes/costs and explicit exclusions through read-only
CLI inspection. [Saved artifacts, matched drift and compaction diagnostics](../evaluations/p6-saved-forecast-diagnostics.md)
retain immutable source-linked evidence with current access and typed retention
([ADR-040](../adr/040-saved-aggregate-forecast-provenance.md)). M3 remains read-only
and unqualified; M4 must record qualification or rejection before serving influence.

Extend the existing report/compare commands with the M1 transition table, sample
denominators and expected visits to retry, handoff, support and verification
states. Show expected total known cost, completion/failure outcomes, uncertainty,
missing liabilities and dominant loops by comparable task/endpoint/policy cohort.
Reports stay useful with zero model budget and no fitted model. Preserve existing
totals and distinguish observed costs from forecasts and causal hypotheses.

Average cost and absorption probability do **not** determine probability of
completion within remaining funds. Initially show expected-cost warnings only.
If a budget-completion probability is later selected here, implement a bounded
cost-distribution/budget-state calculation, validate tail coverage and handle
unknown liabilities explicitly before displaying that claim. No forecast can
reduce protected verification funds or substitute for actual atomic admission.

Add baseline trace log-likelihood/drift and post-compaction transition diagnostics
on matched cohorts, with window/policy/context changes visible. Drift is a review
signal, not proof of a causal regression. Retain source-linked saved reports,
current access checks, and unknown/pruned/small-sample explanations. Acceptance
includes hand-calculated reports, changing cutoffs, no eligible cohort, interrupted
rebuild, late settlement and a read-only CLI trace with no model request or policy
mutation.

## M4 — Qualification and routing estimates

Extend P6-04's declared comparison to four arms: rules only, local statistical
signals, actual Jev through OpenRouter, and the conventional OpenRouter comparator
or explicitly permitted fallback. An optional stronger LLM is an additional arm.
Compare only purposes each implementation supports; a stall-only local producer
does not qualify routing, review or policy proposals by association. Unavailable
live access remains not-run, with no hidden substitute or unauthorized spend.

Freeze project-separated and time-separated tuning/calibration/held-out splits,
labels from current verification/reconciled completion, sample gates, quality
floors, tolerated error rates, uncertainty method and rollback triggers before
evaluation. Private runtime fits stay within their project; multi-project
evaluation uses separately authorized/synthetic datasets without pooling users'
history into a shipping model. Measure forecast error and interval coverage,
Brier/calibration for genuine probabilities, stall precision/recall and serious
misses, plus completed/failed-task total cost, interventions, latency and local
CPU/memory/inspection overhead. Shadow agreement is not task-level benefit.

After acceptable evidence, refactor `CostEstimate` construction to reference
qualified expected retry/handoff/verification rewards per eligible candidate.
Keep first-request size/price bounds and reservation maxima conservative and
independent from expected path cost. Convert monetary forecasts to checked integer
micros with upward rounding; non-finite/overflow/missing components abstain.
Preserve configured assumptions as the explicit baseline when evidence is absent.
Do not use completion probability to waive a measured quality floor. Record each
estimate component, fit/evidence revision and broader-cohort fallback in routing
inspection. Revalidate immediately before consumption and reservation.

Enable only a recorded per-purpose policy after held-out outcome trials under an
authorized cap. Disabling, sparse evidence, numerical failure, source pruning,
catalog/policy drift or stale input restores the baseline without a remote call
unless independently enabled policy permits one. Verify that costs or probability
cannot widen eligibility, grants, strict pins or limits. P8 repeats every enabled
consumer with actual children and the packaged CLI; insufficient evidence leaves
the candidate disabled with a recorded qualification result.

## M5 — Context, co-change, retrieval and compaction

Under P2-01, refactor optional candidate selection to admit a bounded symbol/file
graph and personalized PageRank scores seeded by permitted objective references,
current diff and diagnostics. Version graph extraction, seeds, damping, iteration
bound and tie-breaking. Handle dangling/disconnected nodes and convergence failure
with declared lexical/path fallback. Bound Git history, files, edges, bytes and
language extraction; unsupported/stale parsers remain visible. Borrowing an
upstream extractor requires the normal pin/license/provenance review first.

Add co-change counts from permitted Git history as a separate navigation source.
Define rename, merge, bulk/generated commit and sparse-history handling; cap hub
bias and do not treat co-change as causal dependence. Missing companion edits
become source-linked notes, never automatic edits or mandatory checks. Candidates
retain current path/root/access/ignore checks and dirty-file revisions. Excluded
or deleted nodes cannot leak names or affect rankings through hidden edges.
Mandatory instructions, unresolved effects and tool pairs keep their priority.

Under P2-08, invalidate graph-derived context on relevant source/Git/authority
changes and preserve its provenance through refresh/compaction. Use M1 to compare
re-reads, repeated failures and useful-file recall before/after matched compaction;
transition drift prompts investigation, not a claim that summaries are sufficient
statistics or permission to remove pinned correctness fields.

Under P5-06/08, evaluate a claim/evidence/file graph as an optional third retrieval
stream only after graph extraction proves useful. Fit no shipping fusion weight
without held-out gains. Filter vertices **and edges** under canonical eligibility
before walking; recheck candidates at return and dispatch. Pin graph/fusion/source
generations and prevent revoked intermediate nodes from influencing a visible
answer. Share graph routines only when concrete consumers need them; no generic
graph service or new store is required in advance.

Acceptance: compare lexical-only, existing map/memory baseline, PageRank map,
co-change and optional memory-graph arms on the same frozen tasks/token budgets.
Measure needed files reaching the prompt, useful context per token, companion
precision, task quality, latency and resource use. Cover unsupported languages,
empty/shallow Git history, cycles, renamed files, dirty/untracked files, junction
escapes, revocation/prune and stale sends. Refresh affected integrated P5-08
evidence; reject the extra source if its burden or false positives outweigh gains.

## M6 — Verification order

Under P2-06, record per-check duration/cost and failure observations bound to check
identity, relevant paths, toolchain/configuration and input fingerprint. Discover
the complete applicable check set before ordering. Among ready independent checks,
evaluate descending estimated failure probability divided by declared positive
execution cost; keep stable ties and baseline order for missing/sparse/invalid
estimates. Honor dependency, isolation, resource and concurrency constraints.
The research's optimality claim assumes independent outcomes and the chosen cost
objective; correlated checks require measurement, not an optimality promise.

An early failure may start repair sooner; unfinished checks remain explicitly
not-run and the final changed revision still needs every required applicable
check. Replanning cannot hide an earlier failure or use a stale pass. No inferred
confidence chooses new commands, suppresses review or proves correctness. A
read-versus-edit heuristic, if evaluated in P7-02 skills, stays an optional prompt
experiment with no scheduler/permission effect.

Acceptance: same required check set and completion result under baseline/reordered
runs, shorter time/cost to first useful failure on held-out changes, stale inputs,
zero duration, unavailable toolchain, correlated/prerequisite checks and failures
after repair. Compare resource overhead as well as latency. Apply the same rules
to P7-05's final integrated-parent verification.

## M7 — Offline proposals and provider bursts

Only after M3/M4 evidence, build a small bounded offline MDP over task class,
capability, counter buckets, trigger and budget bucket. Estimate transitions only
for observed supported actions; unavailable counterfactuals remain unknown.
Bound horizon/iteration and define terminal rewards so retry cycles cannot obtain
unbounded value. Declare completion/quality preferences rather than inventing a
monetary value for success. Include all known supporting costs and uncertainty.

Emit explainable threshold tables and selected-field policy diffs through the
existing preview/apply/rollback path. Hypotheses need matched, capped, explicitly
authorized trials before default selection; correlations under the old policy
are not causal evidence. Revalidate current ceilings and policy base at apply.
Test unsupported actions, censored history, high failure cost, no convergence,
declined suggestions, concurrent policy update and rollback under narrower limits.
Post-publication monitoring uses a new policy cohort, never silently continues the
old fit as though the controlled process were unchanged.

Provider burst modelling is a lower-priority P6-03 extension here: evaluate a
two-state exact-endpoint chain only with adequate recent attempt evidence. Keep
transport outages separate from invalid output/quality failures. Compare backoff,
bounded retry and already-eligible alternative endpoints against existing rules;
do not reset counters, ignore unknown charges or escape a strict pin. Sparse data
or no measured benefit leaves the baseline in place. Synthetic burst fixtures
alone qualify failure handling, not a live endpoint switching default.

## M8–M10 — Reuse and qualification campaigns

P7-04 may reuse qualified M4 forecasts to explain proposed child allocations;
include support, integration and protected final verification without counting
child cost twice. Real atomic root reservations still decide admission, and
insufficient child-specific data abstains. P7-05 always checks the integrated
parent result; P7-06 displays observed versus forecast cost and inferred regime
with provenance. Compare actual delegated tasks under the same root cap before
claiming savings; parent pause/owner loss blocks fresh local and remote work.

Segment 16 begins M9 with invented matrices and fixed PRNG seeds. Generate legal
state/effect traces and deliberately invalid variants, then drive real controller,
scripted provider, storage and retention boundaries. Independent invariants
decide validity; the fitted generator is never its own oracle. Cover long loops,
bursts, forks, late/unknown effects, prune/reopen and restore. Bound run size and
retain shrinkable failing seeds. Do not commit private project counts or traces.
P8 repeats relevant campaigns on both stores, native Windows and packaged actual
delegation, with metrics and failed/not-run evidence in the normal evaluation
records. This supplements existing hand-authored adversarial fixtures.

P10-03's M10 remains after first release. If a regime observer proves useful,
reuse the qualified local filter and durable cursor, bounded queues/debounce,
revision deduplication and current-input validation. Display inferred regimes as
estimates and route proposals through ordinary controller checks. Test storms,
pause/reopen, stale beliefs and disabled observers; no separate always-on model
loop or compulsory remote call is introduced.

## Exit records and exclusions

For every implemented increment, update its owning section and the follow-up
ledger with source/schema revisions, exact commands, linked outcomes, limitations
and remaining gates. M4/M5/M6/M7 qualification publishes the frozen manifest and
predeclared criteria as well as passes, rejected candidates and not-run trials.
No existing complete task becomes evidence for a new signal by inheritance.

Do not build Markov text/code generation, an opaque learned router, production
online reinforcement learning, a general POMDP solver, cross-user uploaded fits
or hidden-state labels presented as facts. A local count-table implementation is
not authorization for a downloaded semantic model. Preserve ADR-020's separate
asset/licensing/qualification decision for that different proposal. Counts remain
local to their permitted scope; introducing retained aggregates beyond pruning
or any broader sharing requires a separate explicit product decision.
