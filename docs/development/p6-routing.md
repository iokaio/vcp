# Routing catalogs, profiles and project optimization

The routing implementation lives in `vcp-models::routing`; canonical publications
and optimization history live in `vcp-lifecycle::foundation::routing_state`.
The coding owner consumes decisions through `foundation::worker::routing`.
Interactive presentation uses `vcp-cli::optimize` and the terminal owner.
These are the logical registry, selector and optimizer boundaries from P6; they
share the existing gateway, artifact store and budget ledger.

The separate [Markov kernels](markov-kernels.md) support the implemented M1
evidence foundation and M3 read-only forecasts. M4 recorded a rejection; these
forecasts do not supply qualified routing estimates.

There are **no live-qualified shipping model groups or measured profile defaults**
in this increment. Synthetic test observations exercise the gates and qualify no
actual endpoint. Legacy explicit single-provider configuration remains usable
without automatic routing. An enabled automatic catalog with no eligible model
stops visibly instead of silently using that legacy provider or another alias.

## Startup configuration

`vcp optimize transitions --from <ms> --until <ms>` inspects retained task-state
transitions without saving a report, changing policy or submitting a model call.
Bounds use inclusive `from` and exclusive `until`; omit them for all currently
retained history before now. `/optimize transitions` provides the same inspection
inside the active terminal. Both work without an automatic routing configuration.
The versioned result exposes per-task observations, counts, source event IDs,
cutoff, current authority/deletion revisions, gaps and censored traces. It omits
objectives and source prose. Blocked/paused are resumable. Missing facts or
revisions break chains, and later current state never fills a historical gap.
Counts are not fitted probabilities or total-cost forecasts. See
[ADR-029](../adr/029-retained-transition-evidence.md) and
[verification/remaining scope](../evaluations/p6-transition-evidence.md).

`vcp optimize observations --from <ms> --until <ms>` and
`/optimize observations` inspect the richer M1 action evidence. Turn, tool/effect
and attempt traces remain separate; retry lineage does not create a fictional transition.
The result shows typed states/phases, exact endpoint/model cohorts, available
task class and hashed check/failure identities. It omits raw commands,
diagnostics, reasons and objectives. Older verification events remain visible
with unavailable task revision/failure signature. See [ADR-030](../adr/030-causal-action-observations.md)
and the [verification record](../evaluations/p6-action-evidence.md).

Action projection version two adds one currency-preserving charge attribution per
attempt. Applied debit/credit settlements remain individually visible, while the
terminal charge is available only from a complete retained prefix ending in a
settled attempt or no-send release. Reserved or uncertain liability has no point
estimate. The projection never adds task/root ledger rollups to their component
attempts. Older usage events without a typed settlement remain readable with
incomplete charge attribution. See
[ADR-031](../adr/031-exact-attempt-charge-attribution.md) and the
[charge-reward evidence](../evaluations/p6-charge-rewards.md).

`routing_state::fits::fit` rebuilds an unqualified first-order task-state
candidate from one authorized transition-evidence view. The artifact records its
source digest/cutoff/window, authority/deletion revisions, observed-state cohort,
counts, raw row samples, prior and minimum-sample gate. Sparse, nonabsorbing or
invalid inputs return a closed abstention. The artifact is never stored and its
qualification/serving fields remain empty/false. See
[ADR-032](../adr/032-rebuildable-markov-fit-artifacts.md) and the
[fit-provenance evidence](../evaluations/p6-fit-provenance.md).

`routing_state::fits::compare` rebuilds the same retained evidence and assigns
whole task identities to a deterministic training or held-out cohort. It compares
first- and second-order next-state likelihood on identical held-out predictions,
penalizes supported free parameters, and checks first-order two-step frequencies.
Sparse or unseen contexts abstain. The comparison artifact is source- and
partition-bound, unpersisted and unqualified. See
[ADR-033](../adr/033-heldout-markov-order-comparison.md) and the
[comparison evidence](../evaluations/p6-heldout-order-comparison.md).

`routing_state::rewards::map` groups attempts into exact augmented model-cycle
cohorts and maps only complete terminal charges to currency-preserving cost
samples. Every role and root/child component stays separate. A cohort with any
unknown terminal charge has no sample mean; available charged and reserved-
liability totals remain visible separately. The source-bound artifact is rebuilt,
unpersisted and unqualified. See
[ADR-034](../adr/034-exact-attempt-reward-mapping.md) and the
[reward-mapping evidence](../evaluations/p6-reward-mapping.md).

`routing_state::consumption::consume_reward` persists the exact scalar selected by
a local optimization inspection only after a fresh source-bound artifact matches.
The immutable receipt binds producer, source, selection, task/attempt identities,
consumer decision and exact currency/micros. `replay_reward` validates receipt
integrity and task access, then returns that recorded value without fitting again.
Receipts are historical-only and cannot serve routing. See
[ADR-035](../adr/035-consumed-statistical-value-replay.md) and the
[replay evidence](../evaluations/p6-consumed-value-replay.md).

Local analysis also works without a provider profile or a model budget:
`vcp optimize status`, `vcp optimize report --from <ms> --until <ms>`,
`vcp optimize answer priority "lower total cost"`, and
`vcp optimize compare <baseline-report> <current-report>`. These commands use the
active canonical owner or open the offline store exclusively. They never start a
task, create a model reservation or enable a provider. Policy preview/apply/rollback
use the active session's explicitly configured trusted ceilings.

Use the existing explicit, user-owned VCP profile outside the workspace and sync
roots. Its optional `routing` member is the serialized
`foundation::routing::Configuration`:

| Field | Required input |
|---|---|
| `catalog` | Sealed `CatalogRevision` containing all candidates, including unavailable and unknown entries with provenance and reasons |
| `policy` | Sealed explicit `Policy`; also supplies trusted startup ceilings for persisted project preferences |
| `task_class` | Exact task-class label used to look up qualified role evidence |
| `estimates` | Per-exact-candidate `CostEstimate` records, with explicit support, child and verification costs and assumptions |
| `raw_catalogs` | Map from qualified snapshot ID to the original endpoint-catalog JSON **string**, preserving its exact bytes |
| `escalation` | Optional explicit retry, quality-switch, decomposition, total-attempt and deadline limits; omission disables automatic quality switching |

Construct revisions using `CatalogRevision::create(...)` and `Policy::seal()`;
do not invent IDs or copy research rankings into live qualification records.
Configuration validation replays `Snapshot::from_endpoints` over each original
metadata string and requires equality with the embedded normalized snapshot.
This verifies the source-to-normalized-field relationship as well as immutable
revision hashes. Live compatibility and role evidence must come from separately
authorized qualification runs; reading configuration never starts those runs.

Start the existing native terminal with the profile, for example:

```powershell
vcp --workspace D:\projects\example --config C:\Users\owner\vcp-profile.json run "Review the requested change"
```

The owner first configures the existing gateway, then validates and publishes the
optional routing configuration. Catalog refresh uses an expected canonical
revision and preserves earlier revisions. The initial project policy is created
only when absent; reopening does not erase subsequent optimization choices.
Effective choices are restricted by the current startup ceilings. A failed
configuration/publication does not turn unknown prices or capabilities into an
eligible free candidate.

All domain timestamps are UTC epoch milliseconds; revisions, token units and
money micros use their existing canonical decimal-string serialization.
Policy quality values use integer basis points, from 0 through 10000. Monetary
arithmetic uses checked integer multiplication and upward rounding, never floats.

## Source-to-field map

| Source or observation | Normalized field | Meaning and failure behavior |
|---|---|---|
| Endpoint catalog `data.id` and exact `endpoints[].tag` | `Candidate.identity`, `Snapshot.compatibility.model/endpoint` | Exact identities must match; renamed aliases inherit no qualification |
| Endpoint status and staged import findings | `Candidate.availability`, `reasons` | Supported/unsupported/unknown are distinct; rejected or missing data remains inspectable |
| Endpoint context, maximum prompt and completion limits | `Snapshot.context/max_input/max_output` | Positive bounded values; actual input and output must fit together |
| Endpoint parameter list plus qualified request contract | `Compatibility.required_parameters`, `Candidate.capabilities` | Capabilities are supported/unsupported/unknown; absent required capability excludes |
| Prompt, completion, caching and request prices | `Snapshot.price.rates` | Exact currency/units; missing required normalization excludes rather than becoming zero |
| Explicit enforced request-price ceiling | `Compatibility.request_price_limit` | Bounds request charges even when catalog request pricing is absent |
| Raw metadata digest, fetch time and source effective date | `Snapshot.raw_sha256`, `Provenance`, `CatalogRevision.observed_at/effective_at` | Observation time and source effective time stay separate; source bytes retained separately |
| Dated exact-endpoint compatibility run | `CompatibilityObservation` | Requires current live supported evidence tied to the exact compatibility ID; scripted/research evidence alone cannot enable dispatch |
| Versioned role/task-class evaluation | `GroupMembership`, `RoleEvidence` | Group, samples, quality, p50/p95 usage and latency retain provenance, dates and limitations; conflicting membership fails eligibility |
| Observed distributions or explicit conservative assumptions | `CostEstimate` | First attempt, retries and handoff usage plus support, children and protected verification are separate; unknown fixed costs exclude |
| Developer configuration and selected optimization edits | `Policy` | Model/endpoint/group permissions, quality floor, sample minimum, freshness, pin/fallback and ordering are explicit |
| Current canonical task/context and ledger | `RoutingInput` | Binds scope/revisions, context identity, required limits, current available funds and verification reserve |

Immutable catalog, policy and decision IDs hash their complete normalized
payloads. Source snapshots retain their original raw metadata identities. A
decision retains every candidate, exclusion codes, chosen exact endpoint,
quality/cost comparisons, evidence references, broader-cohort substitution and
explicit pin fallback. Its reservation field remains absent: final request
assembly and atomic ledger admission are separate operations.

First-attempt estimates must cover the declared input/output bounds; documented
retry, handoff and supporting-work estimates contribute to total task cost.
The host checks the actual serialized selected request and current ledger before
dispatch. Concurrent admissions cannot spend against the selector's earlier
balance. Missing measured task-class evidence excludes unless the policy names a
qualified broader class, and that substitution is recorded.

## Inspect groups

In an interactive session:

```text
/groups
/groups exact/model-id
/groups --offset 8
/next
```

The display shows eight candidates per catalog page, source/age, snapshot
freshness, availability/rejection reasons, compatibility evidence and bounded
per-role group/quality/sample observations. `/next` continues the current text;
the suggested `/groups ... --offset ...` command advances candidate pages.
Large observation lists disclose truncation. The displayed source artifact can
be read with `/read <artifact-id> 0` for original endpoint metadata. Group
membership alone is not eligibility; current policy, role, quality, limits,
prices and root budget are checked again at selection time.

Frontier/High/Medium/Low capability groups are distinct from low/med/high user
profiles. The terminal's explicit profile previews use these lexicographic
comparison orders, with exact model/endpoint identity breaking final ties:

| Profile | Comparison order after eligibility and quality floor |
|---|---|
| low | Total task cost, latency, measured quality, capability group |
| med | Measured quality, total task cost, latency, capability group |
| high | Measured quality, capability group, latency, total task cost |

Eligible support roles use a recorded cost-first ordering under the same quality
gate. This is an explicit deterministic policy, not a claim of measured optimality.

## Optimize project preferences

```text
/optimize
/optimize answer priority Prefer lower total cost for routine changes
/optimize answer size Mostly small edits with occasional larger investigations
/optimize answer review Independent review for risky changes
/optimize answer restrictions No additional model preferences
/optimize preview med --quality-floor <explicit-basis-points>
/optimize preview --models exact/model-id --endpoints exact/endpoint --groups high,medium
/optimize preview --pin exact/model-id exact/endpoint
/optimize preview --minimum-samples 30 --maximum-evidence-age-ms 86400000
/optimize preview --output-tokens 512
/optimize apply
/optimize status
/optimize rollback <target-revision> --expected <current-revision>
```

Replace the placeholders with selected values; no quality threshold is inferred
from interview prose. Report generation is local and counts retained failures,
cancellations, unfinished work, retries, support work, known spend and uncertain
liability. Coverage limitations and unknown abandonment remain explicit. The
history window selects tasks; outcome/cost summaries describe their retained
canonical lifetime at the report cutoff rather than pretending to reconstruct
historical state from current rows.

Questions adapt to report gaps and already saved answers. Optional questions may
be answered directly even when not prompted. Answers are durable project
preferences, not authority or automatic model restrictions. Explicit selected
model/endpoint/group restrictions and pins use the preview/apply workflow within
the startup configuration's trusted ceilings. Reporting and answers remain
available when automatic routing is not configured; preview/apply requires the
configured policy and ceilings.

The positional preview selects profile, comparison ordering and the supplied
quality floor. The flag form changes only selected fields: `--profile` (including
its ordering), `--quality-floor`, `--output-tokens`, `--input-tokens`, `--minimum-samples`,
`--maximum-evidence-age-ms`, `--models`, `--endpoints`, `--groups`, `--pin` or
`--unpin`. Lists are comma-separated, contain at most 32 distinct IDs and use
`none` for an empty deny-all selection. Group names are frontier/high/medium/low;
profile names are low/med/high. A pin is strict and names both model and endpoint.
Removing a project pin cannot remove a trusted startup pin. Conflicting strict
pins produce no eligible model. Values outside trusted allowlists remain visible
in the requested policy but are excluded from its effective policy.

`--output-tokens N` selects a positive request output limit; `inherit` clears
the project preference. The effective value is bounded by both trusted routing
configuration and the immutable host ceiling. The selected value is used by the
actual provider envelope, serialized request and reservation quote. An admitted
request retains its captured limit when later preferences change. Old policy JSON
without this optional field retains its original digest and remains readable.

`--input-tokens N|inherit` selects the existing conservative context-size ceiling.
The current host measures serialized request bytes; this is not a new tokenizer
or a promise of exact token counting. Selection, sealed-context validation and
admission enforce the smaller selected/trusted ceiling. Providers without a
qualified byte-to-token bound retain full provider-input-capacity reservations.
An oversized request is rejected before a provider attempt is admitted.

`--max-transport-retries N|inherit`, `--max-quality-switches N|inherit`,
`--max-total-attempts N|inherit` and `--minimum-repeated-failures N|inherit`
narrow an already configured escalation policy. Maxima can only decrease;
the minimum failure count can only increase. Ranges are 0–4 retries, 0–8 switches,
1–64 attempts and 1–64 repeated failures; the host's trusted startup retry cap
also applies. These selections cannot enable missing escalation configuration,
extend its deadline or grant decomposition permission. Clearing a selection
restores the current trusted value, including after rollback.

`--reasoning-effort minimal|low|medium|high|inherit` selects an effort level up to
the explicitly configured trusted level. Without that trusted level the effective
selection remains unset. A candidate must separately qualify the requested effort
and advertise the provider parameter; unsupported candidates remain ineligible,
including strict pins. The provider boundary emits and validates the selected
parameter. This does not infer supported levels from a model name.

Trusted startup profiles can also set `"output_tokens":"512"` and
`"max_transport_retries":0` for fixed-provider trials. Omitting those fields
preserves the existing 4096-token output allowance (bounded by provider capacity)
and two transport retries. An explicit output allowance can be 1..16384 tokens,
clamped to the qualified provider maximum. This is total output, including any
reasoning tokens; it does not reserve a separate answer quota or select reasoning
effort. Startup rejects zero or above-16384 output limits and more than two
retries. A selected policy can only narrow these trusted limits. Evaluation plans
freeze their explicit allowance; changing it requires a fresh plan and does not
change historical results.

Reasoning-heavy fixed-provider trials can also explicitly set
`"provider_timeout_seconds":360`. Omitting it preserves the 120-second response
timeout. Explicit values must be 1..360 and no greater than `deadline_seconds`;
every in-flight response is also bounded by the remaining configured coding deadline.
This does not extend the overall task deadline, enable retries or change process,
MCP or decision-service timeouts. An interrupted submitted response retains its
unknown-charge reservation until reconciled.

`--retrieval-limits RESULTS TOKENS BYTES` restricts explicit memory-context
queries; `inherit` clears the project preference. Each bound is clamped to the
trusted policy and the retrieval service's hard limits. The token field uses the
retrieval service's serialized-passage byte estimate. It does not enable memory
search, expand its source scope, load an embedding model or change read-only
inspector queries. The ordinary coding loop does not automatically call this
explicit query API. A policy change during a query or after context preparation
invalidates its result before it can be sent. Reopening requires the trusted
routing configuration before a stored routing policy can govern new queries.

Preview shows previous, requested and effective values for every selected field.
Omitted fields retain their current values; no threshold is inferred from prose.
The pending
typed preview remains local to this terminal session; applying sends exactly that
preview with an idempotent command identity. A changed policy, report access,
interview or trusted ceiling causes revalidation to reject stale work. An attempted
replacement clears the previous pending preview even if replacement fails.
Reopening requires a new preview before apply. Rollback explicitly names both
target and expected current revisions, creates a new revision, and reapplies
current trusted limits.

These controls make no model calls, spend no evaluation budget, change no task
authority and do not implicitly resume paused work. Active requests keep their
captured policy; subsequent work revalidates at the scheduling boundary. Paid
qualification, optional semantic evaluators and measured P6 profile defaults
remain separate gates.

## Explicit escalation declarations

In the current owner terminal, use `/escalate complexity --evidence ARTIFACT,...`
or `/escalate capability NAME --evidence ARTIFACT,...`. Evidence must identify
complete retained artifacts from the current task. Capability names must have an
explicitly supported candidate in the current catalog. The terminal binds the
declaration to the current task revision and steering; the host checks current
owner, running task/ancestors, authority, workspace binding and prior model attempt.
Declarations do not resume work or grant a model permission.

At the next scheduling boundary the host consumes a matching declaration once,
adds any required capability to candidate filtering, and uses the existing bounded
escalation and handoff path. Strict pins, eligibility, attempt limits and budget
still apply. A failure after consumption requires a new explicit declaration;
reopening cannot replay a consumed declaration. An admitted capability remains
required for subsequent requests in the same task and steering revision.
`/escalate status` distinguishes pending, stale, superseded,
consumed-without-admission and admitted records. Inspection and consumption use
the same authority, binding, revision and evidence checks. Consumption alone is
not evidence of an admitted or completed handoff. Automatic failed-verification and invalid-tool triggers
remain available independently of these owner declarations.

## Verification entry points

`/optimize compare <baseline-report> <current-report>` reloads both reports under
current access and compares declared cohorts and source windows. Counts retain
their denominators; unknown charges prevent spend-improvement claims. Observed
verification checks and submission-to-usage latency remain distinct from model
quality and task duration. Comparisons can recommend reviewing a rollback but
never apply one or authorize another trial.

`vcp-models/tests/routing.rs` covers deterministic eligibility, conservative cost
arithmetic, pins/fallbacks, immutable refresh/reopen and ordering. Canonical routing
state tests cover publication/CAS, report access, saved answers, previews,
idempotent apply and rollback. `vcp-cli::optimize::tests` covers the human command
grammar, adaptive interview, requested/effective preview, explicit apply, retry
identity and paged group display. Native owner tests verify real dispatch/admission
integration separately; parser tests alone do not qualify a live model or native
console workflow.

The first P6-03/M2 lifecycle increment adds
[canonical escalation-advisory records](../evaluations/p6-advisory-records.md).
Prepared requests and decoded results are immutable, deduplicated and task scoped;
late input/deadline results remain historical evidence. Caller-owned scheduling,
pause/cancel and remote accounting remain the next increment.

The [caller-owned advisory scheduling lease](../evaluations/p6-advisory-scheduling.md)
now adds a single revisioned claim, dispatch-time pause/stale/deadline checks,
explicit interruption and reopen without replay. It remains transport-free; the
subsequent accounting binding reuses ordinary helper reservations.

The [advisory accounting binding](../evaluations/p6-advisory-accounting.md) now
connects an active claim to a retained typed request artifact and ordinary helper
attempt. Budget submission, uncertainty and settlement stay canonical. The actual
caller-owned transport/response path remains next.
