# Model rotation, capacity recovery and accounting plan

Status: owner-approved October 3, 2026; implementation requested on the existing
`fix/concurrent-provider-admission` branch, to be renamed `feature/model-rotation`
before push and PR creation. Implementation and qualification status below must
describe actual evidence rather than treating this plan as passing acceptance.

## Authority and decisions

The owner approved the following behavior:

- Select first-, second- and third-choice **sets** of models. Rotate on each
  model request inside the highest-priority eligible set with ready capacity;
  use later sets when earlier ones cannot serve the request.
- Use similarly capable, similarly priced members. Suggest normal-set members
  within 2x a reference request's cost; an explicitly selected stronger reserve
  set has its own ceiling. Show exact endpoint tariffs and conservative
  reservation requirements. Selection is owner preference, not proof of equal
  model quality.
- Automatically use explicitly approved later sets within their ceilings and the
  existing hard task budget. Outside models and endpoints remain unauthorized.
- Initially use OpenRouter, diversifying both models and exact endpoints.
- When all approved routes are cooling down, wait within the original deadline,
  reconcile outstanding charges and allow one coordinated recovery probe.
- Settle provider-confirmed charges, including confirmed zero. Retain unknown
  liabilities; contain repeated failures before they consume usable balance.
- Perform offline qualification first. The complete live compatibility and
  concurrent-scenario campaign has a **$25 total** ceiling, including settled,
  active and unresolved costs. This authorizes no deployment or release.

Account/project preferences affect future tasks. Tasks capture their selected
sets and policy. Existing fixed selections, strict pins and legacy ordered
alternatives preserve their behavior; an existing task adopts a new selection
only through an explicit authorized update. Privacy, tools, sources, capability,
context, price, budget, deadline and request-count boundaries remain mandatory.

## Evidence and limits

The owner's scenario analysis reports 167 submitted requests, 16 HTTP 429s and
4,520,415 input tokens over approximately 9.1 minutes. The 151 successful requests
matched provider request identities; rejected requests have no token counts.
Both scenarios used `qwen/qwen3-coder` on `google-vertex/us-south1`; structured
failure metadata identified `upstream_provider_shared_pool`. Context grew from
approximately 7k to 44k/71k tokens. Only about 8% of observed input was cached.

The data suggests token pressure but does not establish the upstream TPM limit,
prove concurrency caused the failures, or reveal other customers' load. At most
two requests were in flight; a two-request global cap alone does not address
these observations. The rejected requests' token consumption remains unknown.

For A-T2, approximately $0.33 settled while 14 uncertain requests retained about
$2.64 in worst-case liability under a $3 cap. A rapid rejected response without
output is not proof of zero charge. B's build/completion failure is separate;
routing must not weaken verification or replay uncertain tool effects.

Existing runtime evidence:

- Owner-assigned retry alternatives exist for rate-limit/transient failures, but
  selection is first-eligible rather than round robin.
- Setup retains one selected endpoint per model, losing eligible diversity.
- Deterministic complete-tool-pair compaction exists; initial inspection found no
  production CLI call enabling continuity.
- The ledger supports idempotent late observations/reconciliation. Successful
  probe attribution cannot serve as a failed-request charge parser.
- ADR-083's shared pacing provides useful asynchronous cancellation, deadline and
  safe filesystem boundaries, but its all-model cooldown delays healthy routes.

## Work items and dependencies

These focused follow-ups extend completed owners rather than changing their
historical acceptance evidence. Each item must retain reviewable implementation
and actual verification records.

| ID | Owning contracts | Increment | Dependencies |
| --- | --- | --- | --- |
| MR-01 | BETA-03D, P6-01/02 | Three choice sets, cost policy, endpoint discovery and compatibility | Existing owner selections/catalog admission |
| MR-02 | P2-08 | Production continuity/compaction integration | Existing deterministic compaction |
| MR-03 | P6-02, P2-02 | Shared round robin, route health, token-aware capacity and recovery probes | MR-01 |
| MR-04 | P2-02, P6-03 | Scoped failover, reassembly and canonical admission | MR-01, MR-02, MR-03 |
| MR-05 | P1-05, P2-02 | Failed-request receipt reconciliation and liability containment | Existing ledger; integrate MR-04 |
| MR-06 | P6-04/05, BETA-03C/D | Inspection, offline qualification and capped live scenario evidence | MR-01 through MR-05 |

## Selection and coordination

Represent up to three ordered choice sets for each applicable role. Each contains
bounded approved models and exact allowed endpoints plus its cost ceiling. Model
selection precedes endpoint selection, so a model with more endpoints does not
receive disproportionate traffic. Normal-set suggestions compare the same
representative prompt/output against captured endpoint prices, including cache
classes; live admission still checks the actual request and conservative quote.

For every model step:

1. Filter captured membership against current mandatory restrictions and budget.
2. Look for eligible ready capacity in the first set, then subsequent sets.
3. Select the next model and endpoint using coordinated round-robin positions.
4. Reassemble for the selected model, including tools and output/context reserve.
5. Validate the serialized request and obtain atomic canonical budget admission.
6. Capture the selection, policy/state references and attempt; dispatch once.

Coordinate local processes independently of their workspace/data directories.
Selection positions advance once for a dispatch selection, not during repeated
eligibility validation. Recorded choices remain reproducible. Shared state
contains bounded nonsecret capacity/health facts; it cannot grant authority,
share task content or replace canonical ledgers. Waiting creates no billable
attempt and holds no canonical store transaction. Cancellation/current-owner,
source revisions and the original absolute deadline remain effective.

Use exact model/endpoint availability scopes and conservatively recorded shared
capacity associations. Different names/regions are not proof of independent
infrastructure. Retain a bounded overall resource ceiling without using one
failed model's cooldown to block unrelated healthy routes.

## Recovery and failure policy

Normalize allowlisted structured `limit_source`, error type, HTTP/SSE status and
Retry-After at the provider boundary. Do not branch on arbitrary remedy prose.

| Failure | Response |
| --- | --- |
| Upstream shared-pool 429 | Exclude/cool down affected capacity and try an approved healthy alternative |
| Provider overload/eligible 5xx | Bounded failover, temporary exclusion and backoff |
| OpenRouter in-flight spending limit | Account-wide wait and reconciliation |
| Key/credits/authentication | Stop with an actionable account diagnosis; no blind rotation |
| Invalid request/capability/policy failure | Explain/reject; no automatic cycling |
| Timeout/disconnect/partial stream | Preserve uncertainty and use only a supported recovery/continuation boundary |

Honor supported Retry-After for affected capacity. A healthy independent route
need not inherit an endpoint cooldown; account-wide constraints still apply.
First overload opens a cooldown; repeated failures extend exclusion. Only one
process owns a post-cooldown recovery probe. Success restores eligibility; the
next independent model step returns to the highest-priority usable set.

Every failover has a new linked reservation. Set/endpoint changes never reset
retry count, task request allowance, original deadline or money limits. Start
with the existing two-retry default: at most three submissions per model step.
Skip ineligible routes without submitting. Never dispatch incomplete tool calls,
replay uncertain effects or accept stale responses after steering/revocation.

Reassemble cross-model context from objective, current constraints/decisions,
source/diff state, checks, pending effects, complete tool pairs and current money
state. Validate destination context, tool support, reasoning parameters, privacy
and price; record incompatible opaque provider fields instead of inventing them.

## Context and token pressure

Enable existing deterministic continuity for new/resumed/delegated CLI tasks.
Compact older completed tool pairs into bounded previews and retained-artifact
references. Keep authoritative current facts and unfinished pairs outside the
summary; preserve original history and access/source-revision checks.

Measure actual serialized reduction and introduce earlier bounded working-context
targets. Recheck fit for smaller destinations. Stop repeated no-gain compaction.
Preserve stable prompt prefixes where practical and report observed cache use.
Per-request rotation may sacrifice endpoint-local caching; cache savings are not
proof of corresponding TPM discounts. Scheduling estimates are not monetary
bounds: conservative reservations cannot shrink without trustworthy envelope
evidence.

Track rolling route requests, estimated input/output load, actual usage/cache
counts and queue times. Prefer round robin among routes with headroom. Use known
provider limits when available, otherwise labeled local targets and bounded
adaptation to observations. Prevent starvation of large eligible requests.

## Accounting and reconciliation

Capture bounded failed-request identities and raw evidence. Retrieve authoritative
request/generation charge metadata without repeating inference. Validate attempt,
request identity, model/endpoint attribution where available, currency, exact
nonnegative final charge and provenance before submitting an idempotent late
observation to the existing ledger.

Confirmed zero settles zero. Missing identity/cost, unavailable metadata, malformed
or conflicting receipts and HTTP 404 preserve unknown liability. Repeated receipts
cannot release twice; receipt capture/settlement survives crashes on both stores.
Do not weaken successful-probe attribution to accommodate failed requests.

Contain repeated unresolved availability failures across successive steps. Pause
new inference and prioritize reconciliation at a bounded failure threshold or
when another eligible attempt cannot preserve protected verification balance.
Calibrate the threshold with replay evidence. Display settled, active, unresolved
and spendable amounts separately. Late reconciliation updates current balance,
without rewriting past failure evidence or resetting consumed request counts.

## Candidate investigation

Evaluate current exact endpoints for Qwen3 Coder, DeepSeek V3.2, GLM 4.7 and Kimi
K2.5 as an initial shortlist, not approved equivalent-quality claims or immutable
defaults. Require adapter/tools/context/privacy compatibility, current tariffs and
representative VCP behavior. Investigate same-model hosting diversity as well as
different model makers/backends. Catalog/benchmark visibility alone does not
establish interchangeability or independent capacity.

## Qualification and delivery

Offline tests exercise actual production CLI setup and independent processes:
rotation/tier fairness, endpoint weighting, scoped cooldown, all-unavailable waits,
single recovery probes, token pressure, strict pins, cancellation/deadline with
zero queued reservations, source/privacy narrowing, smaller contexts, resume,
partial streams/late replies, ledger bounds and both stores. Compaction retains
late corrections, required checks, unfinished pairs and unknown effects. Receipt
tests cover zero/nonzero charges, missing IDs/costs, conflicts, duplicates, 404,
malformation and crash recovery.

After offline checks pass, use a source-bound campaign with explicit aggregate
accounting under the $25 cap. Compare the prior policy and combined policy at the
same concurrency. Record build/task correctness, 429s, input TPM, cache reuse,
failovers, latency, settled cost and unresolved liability. Preserve enough budget
for verification; stop admission when settled+active+unresolved consume the cap.
Report unavailable checks honestly. Scripted tests do not prove live capacity.

Review/format/test each increment, preserve upstream patches/provenance, commit,
rename the branch before push, and open a PR. Observe required checks/reviews and
respect branch protection. Any new distributable candidate requires synchronized
version increments and actual artifact-version verification under AGENTS.md.

## Implementation evidence

Pending. Record commands, results, limitations and live spend here or in a linked
source-bound evaluation before marking an item complete. No plan statement is
itself verification.
