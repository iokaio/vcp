# ADR-002 — Internal commands and deferred public protocol

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P1-02; later P9-01/02/03. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#5-engine-api-and-protocol) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Typed internal commands/events serve both CLI modes now. Public JSON-RPC, general client attachment and generated SDK types are deferred until P8-05. A qualified private owner-control path may deliver CLI pause/resume commands without exposing that later public API or opening another writer. All surfaces invoke the same controller, policy and ledger.

## Implementation proposal

Keep durable command identity separate from transport request identity. Bind payload digest, expected revision, caller and workspace to each mutation. Persist its result before acknowledgement; duplicate delivery returns that result. Event cursors carry scope and sequence and return a gap plus snapshot when retention prevents replay.

Detailed contracts and failure ordering are in the [supporting design](../architecture/deferred-clients-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Use internal Rust contracts without freezing a public wire schema during P0/P1. P9 selects framing, negotiation and local authentication from real transport fixtures. Generated TypeScript comes from one authoritative schema, with drift checks.

## Qualification evidence

P1 selects internal envelope version 1, typed UUID identities, decimal-string
unsigned counters and a recursively canonicalized whole-envelope digest. Current
access is checked before receipt lookup; matching durable receipts precede stale
revision rejection. Bounded pull subscriptions declare snapshot and gap behavior.
The [foundation guide](../development/p1-foundation.md) specifies these private
formats and [P1 qualification](../evaluations/p1-completion.md) records native
interactive/JSONL parity, restart and retained-consumer acceptance. Public wire
negotiation, SDK generation and transport authentication remain P9 decisions.

E01/E09/R01 cover CLI parity now and API parity later. Test stale answers, duplicate payload mismatch, counters beyond JavaScript precision, slow subscribers and reconnect after commit-before-reply. Record the supported version and enum-evolution rules before advertising an SDK.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Earlier daemon-continuation wording in architecture §5.4 and §18.5 has been corrected to match the confirmed §4.5 close-to-pause requirement. Detachment is not background authorization. Preserve close-to-pause; any future independent owner mode needs an explicit product decision. Observer disconnect alone does not stop a different live owner.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
