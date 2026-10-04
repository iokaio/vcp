# ADR-084 — Owner model rotation and scoped capacity recovery

Status: accepted October 3, 2026 for MR-01 through MR-06. Implementation and
verification are tracked in [the model rotation plan](../architecture/model-rotation-plan.md).

## Decision

New explicitly selected rotation policies capture up to three ordered choice
sets per role. Dispatch rotates models inside the first eligible set with ready
capacity; later approved sets are available automatically within their captured
price ceilings and the canonical root budget. Endpoint selection is a separate
rotation so models with more hosting endpoints do not receive extra model turns.
An owner selection is permission and preference, never empirical quality proof.

This extends ADR-082's model selections with an explicit rotation mode. Existing
legacy ordered alternatives, retained memberships and strict pins keep their
prior behavior. Metadata refresh may narrow or renew captured exact endpoints;
it may not add outside identities to a retained task.

Shared user-local coordination records bounded route availability, selection
positions and locally estimated token pressure. It holds no workspace authority,
canonical money state, project content or credentials. A route-specific overload
cannot block independent healthy alternatives. Account/credential failures remain
account-scoped. Different model/provider names alone do not establish independent
upstream capacity. This supersedes ADR-083's all-model cooldown for explicitly
rotating dispatch while preserving its safe asynchronous admission, process-death
lease release and filesystem protections.

Every transmitted attempt has its own canonical reservation and predecessor
link. Switching endpoints, models or sets never resets retries, deadlines,
request counts or spending bounds. Preparation/revalidation do not advance a
selection twice. Waiting and cancelled queues create no billable attempt.
Only a coordinated recovery probe may re-enter a cooling route after expiry.

Cross-model assembly carries current authoritative task facts, source versions,
complete required tool pairs, effects, checks and accounting under the receiving
model's envelope. Production CLI tasks enable deterministic historical tool-pair
compaction; isolated children inherit projection policy, never parent history.
Original capture and source/access validation remain authoritative. Scheduling
estimates and cache observations do not become qualified financial bounds.

Failed requests settle only from qualified final charge evidence. Dedicated
failed-request receipt parsing does not weaken successful-probe attribution.
Missing IDs, absent/unavailable receipts and lookup 404 retain uncertainty.
Reconciliation reuses the canonical ledger's idempotent late observations.
Repeated uncertain availability failures are contained before further inference
can consume usable balance or protected verification funds.

## Alternatives and consequences

A global two-request limit does not resolve a workload already limited to two
in-flight requests. Blind retry on the same saturated endpoint increases unknown
liabilities. Gateway-controlled fallback obscures attempt accounting and exact
selection, so VCP retains dispatch control and disables hidden fallback/retries.

Per-request rotation distributes traffic more rapidly but can reduce endpoint
cache locality and introduces cross-model behavior differences. The owner chose
this tradeoff. Current endpoint metadata and bounded VCP compatibility/behavior
tests constrain selection; no universal equivalence or available-capacity claim
follows from catalogs, benchmark scores or scripted tests.

Normal member suggestions use a common request envelope and a 2x reference-cost
ceiling; an explicitly selected stronger reserve has a separate ceiling. Actual
admission still checks current snapshot prices and conservative liability.
The approved live qualification campaign has a $25 aggregate cap including
settled, active and unknown costs. No deployment or release is authorized here.
