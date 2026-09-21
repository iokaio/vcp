# Routing catalogs, profiles and project optimization

The routing implementation lives in `vcp-models::routing`; canonical publications
and optimization history live in `vcp-lifecycle::foundation::routing_state`.
The coding owner consumes decisions through `foundation::worker::routing`.
Interactive presentation uses `vcp-cli::optimize` and the terminal owner.
These are the logical registry, selector and optimizer boundaries from P6; they
share the existing gateway, artifact store and budget ledger.

The separate [Markov kernels](markov-kernels.md) provide bounded pure arithmetic
for M1. They do not yet supply history-backed routing estimates or forecasts.

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
preferences, not authority or automatic model restrictions: changed model/provider
allowlists still require trusted configuration. Reporting and answers remain
available when automatic routing is not configured; preview/apply requires the
configured policy and ceilings.

Preview selects profile, comparison ordering and the supplied quality floor. It
shows previous, requested and effective values under trusted limits. The pending
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
