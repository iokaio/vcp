# ADR-035 — Consumed statistical value retention and replay

Status: selected for the final P6-02/M1 foundation increment. This defines an
immutable receipt for a statistical scalar actually selected by local analysis.
It does not qualify that scalar or enable a routing consumer.

## Evidence and problem

Rebuilding a fit during replay can change a past decision when history, retention,
authority or producer code changes. Saving an entire fitted artifact would create
a stale cache that can outlive pruned inputs. M1 instead requires the exact value
that a consumer used, the producer and input identities, and the consumer decision
to be retained together.

## Decision

Add `routing_state::consumption::consume_reward` for the first concrete local
consumer purpose, `optimization_inspection`. It accepts a reward artifact and an
exact cohort/currency selection. Before writing, it rebuilds the artifact under
current access and requires complete equality. Abstained artifacts and cells with
an unknown remainder cannot be consumed.

The immutable `vcp_consumed_reward_v1` receipt records:

- workspace, authority/deletion revision, consumer command and timestamp;
- producer version and artifact, source evidence/digest/window/cutoff;
- exact cohort/currency selection and its digest;
- the sorted source-attempt digest and bounded source-task scope;
- one canonical input digest over producer, source, selection and scalar;
- the exact upward-rounded consumed micros/currency; and
- current policy/catalog identities plus explicit historical-only and
  non-serving status.

The projection record references every selected task, attempt and retained
settlement so canonical retention closure can see its dependencies. It contains
no source prose and no full fit. Reusing a consumer decision is idempotent only
for the same producer artifact and selection; conflicting reuse fails.

`replay_reward` validates the immutable receipt and access to every source task,
then returns the recorded scalar. It deliberately does not rebuild a current
artifact. A new consumer decision always revalidates a fresh artifact, so replay
cannot turn a historical value into current serving authority. Narrow task access
cannot read a receipt that aggregated another task.

## Consequences

Historical optimization inspection can reproduce the value it actually used even
after later canonical observations change the current fit. Pruning and retention
planning can follow explicit canonical references; any retained receipt is bounded
historical metadata, not a resurrected fit or source payload.

This completes the M1 retained evidence and analysis foundation. All statistical
artifacts and receipts remain unqualified and outside runtime routing. M2 owns the
next P6-03 local stall-shadow integration; M3/M4 own forecasts, qualification and
any enabled consumption. Verification is recorded in the
[increment evidence](../evaluations/p6-consumed-value-replay.md).
