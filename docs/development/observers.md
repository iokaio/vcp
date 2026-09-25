# Optional local observers

P10-03 selects one explicit observer: `exact-verification-repetition/1`.
The first version observes the owning root task. It reports repeated
failed-verification patterns on the same task, steering and
input, with references to the retained verifications. It does not diagnose a
stall or recommend automatically retrying, stopping, changing permissions or
relaxing required checks. Productive work and environment failures can repeat.

Observers are disabled when the native owner profile omits `observers`.
An explicit owner profile may opt in:

```json
"observers": {
  "enabled": true,
  "limits": {
    "max_attempts": 16,
    "steps_per_attempt": 4096,
    "max_total_steps": 65536,
    "deadline_ms": 2000,
    "debounce_ms": 100
  }
}
```

These are ceilings on local observation work, not model allocations. The observer
has no provider-dispatch path and does not add a paid request. Root budget and
owner admission still gate work. Imported foreign configuration cannot enable it.
The initial capture conservatively refuses a canonical snapshot larger than its
step ceiling or 4 MiB source-byte ceiling; streaming capture checks its deadline
without materializing an oversized snapshot. It does not analyze a truncated history. Narrower history
indexing is a future optimization, not permission to exceed the limit.

Use `/observers` in the owning terminal to read activity, retained proposals and
local cost, then `/next` for additional pages. Current and historical advice are
distinguished. Status remains readable while paused and does not schedule work.
Normal `/pause` and owner loss stop fresh scheduling; deliberate `/resume` uses
the existing task, policy, budget and effect reconciliation checks. Reopened
outstanding work is interrupted, not silently replayed.

One coalescing slot and one outstanding computation bound each root observation
state. Duplicate input reuses its prior attempt; a later identical repetition
does not create a second notice merely because its watermark advanced. Attempt
and step limits are durable, so reopening does not replenish the allowance.
Source changes or user corrections invalidate current advice. Late completion
cannot apply to the replacement task revision.
Forgetting contributing evidence redacts the root's derived observer record.
Its content-free tombstone makes that observer unavailable for the existing root;
reconfiguration cannot recreate its allowance or replay the forgotten input.

The owner prepares authorized retained input, performs bounded local computation,
and revalidates before publishing advice. Observations are durable local tasks
under the root, without the executable authority of a delegated model child.
No independent timer loop runs, and no inferred regime is presented as a fact.
Reported local elapsed time covers preparation, debounce and computation; it is
not CPU utilization and excludes the final publication transaction. Qualification
also measures the complete poll and retained bytes. No model cost is substituted
for these local resource measurements.

The [grading plan](../evaluations/p10-03-observer-grading.md) measures factual
accuracy and grouped evidence access, with full local overhead and zero-provider
fixtures. It does not establish human time savings or production completion/cost
improvements. The feature remains opt-in. The statistical M10 candidate and
additional observers require their own qualification.

See [ADR-067](../adr/067-bounded-local-observation-tasks.md) for the local-work
contract and the distinction from model-backed delegation.

Run `./scripts/test-observers.ps1` on native Windows for the recorded qualification
suite. See the [evidence record](../evaluations/p10-03-observers.md) for measured
results, recovery scope and limitations.
