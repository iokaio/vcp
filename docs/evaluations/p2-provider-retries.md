# P2-02 — Accounted retained provider retries

The retained HTTP controller now asks its canonical model permit to qualify a
retry. The client still disables implicit HTTP, stream and authentication retries.
Eligible rate-limit, transient HTTP and transport failures receive at most two
VCP retries within the original absolute request deadline. Rate limits use a
5/10-second exponential cooldown and a 60-second individual delay ceiling;
transient failures retain the 100/200 ms delay and 5-second ceiling. Qualified
`Retry-After` delta seconds can increase the delay. Unsupported, overflowing,
excessive or deadline-exceeding values decline the retry rather than allowing an
early request. No cooldown changes an explicit model/provider pin.

Each retry captures its own exact request and reserves a new attempt with a
predecessor link. Submitted failures keep their independent unresolved liability;
a depleted budget prevents the next send. A positively local response-spool
failure before send intent releases the undispatched reservation. Non-success
HTTP bodies are captured as raw evidence and never parsed as successful SSE.
Interrupted or malformed SSE is not automatically replayed and cannot release
unresolved liability or dispatch incomplete tool calls.

HTTP failures retain a `provider-failure/1` artifact containing the transport
status, failure class, allowlisted limit source, retry hint, attempt ID and raw
response artifact ID. Durable task/attempt reasons expose the normalized cause
even when retries are exhausted. Raw messages, account IDs, provider names and
remedy prose are not copied into those status reasons. Neither an HTTP status
nor a provider error body is a qualified zero-cost receipt: all submitted
liabilities remain unresolved until ordinary accounting reconciliation.

This correction follows the October 3, 2026 A/B CLI failures. Both runs retained
three HTTP 429 responses identifying `upstream_provider_shared_pool`; the old
100/200 ms policy exhausted retries within a few seconds and surfaced only a
generic accounting pause. Project isolation does not isolate upstream capacity.
The cooldown is bounded recovery behavior, not a claim that the endpoint will
be available after waiting. It follows OpenRouter's documented
[rate-limit backoff and Retry-After guidance](https://openrouter.ai/docs/api_reference/limits).

The retained header boundary rejects an already-expired carried deadline before
polling transport. This check is explicit because Tokio's timeout wrapper polls
its inner future before checking its timer. The focused retained core regression
`vcp_expired_header_deadline_never_polls_transport` uses an immediately ready
transport future and requires that it is never polled after expiry.

Backoff holds no controller/store lock. Retained pause generations, canonical
owner/task state, authority, sources and the deadline are checked throughout the
wait and again before admission. Dropping the timer cancels its pending retry.
Reopening a store does not restore retry authority. Coding retries reassemble
continuity and explicit handoff packets from the newly retained liability before
fresh admission; immutable model/provider/data restrictions remain enforced.

## Evidence

`src/crates/vcp-lifecycle/tests/support/provider_retries.rs` drives the actual
retained HTTP client and canonical gateway against an independent scripted
observer, on both SQLite and file stores. Its cases cover:

- 429 followed by success, distinct reservations, predecessor links and retained
  earlier liability;
- 503 exhaustion, authentication rejection, excessive/unqualified delay,
  insufficient deadline and depleted budget;
- exact deadline identity across fresh attempt admission and a delayed second
  response bounded by that deadline;
- pause, owner close and steering during backoff;
- coding with continuity enabled, current accounting in the retry body, handoff
  remaining budget changing from 1000 to 900 micro-units, and an SSE-shaped 429
  body that cannot manufacture a successful settlement.
- a sanitized replay of the observed shared-pool 429 JSON on both stores:
  cooldown before the only permitted retry, actionable persisted failure reason,
  exact raw evidence links, no tool effect or settlement, and independently
  retained liabilities for both attempts.

The focused native command is:

```powershell
cargo +1.98.0 test -p vcp-lifecycle --test canonical_host --features qualification provider_retry --locked --target x86_64-pc-windows-msvc -j 4 -- --test-threads=1
```

Run from the retained Cargo workspace with the repository's native Visual C++
environment and external target directory. This is scripted control-flow evidence;
the dated live compatibility qualification remains recorded separately.

The three named native retry contracts passed in focused runs on 2026-09-19.
The matrix's initially short 1.5-second delayed-response fixture could expire
before a second dispatch during concurrent native compilation. It also passed
unchanged when run alone. The final fixture leaves four seconds for dispatch and
delays the response eight seconds; a separate exact-`Instant` assertion checks
that fresh admission never resets the deadline. Repository integration reruns
these final inputs alongside the other P2 contracts.

The September evidence above describes the prior retry timing. The October
correction adds pure rate-limit deadline/hint bounds and safe diagnostic parsing
tests, and changes the carried-deadline fixture to eight seconds with a
twelve-second response. On October 3, 2026, all 22 `vcp-models --test provider`
tests passed, followed by all six `canonical_host --features qualification`
tests matching `provider_retry` (178.47 seconds, zero failures or ignored tests).
The latter includes the seeded pause/reopen no-replay contract and both-store
regressions proving that oversized or malformed HTTP bodies cannot qualify
metadata from an individual capture chunk. The full bodies remain retained and
their liabilities remain unresolved. The native loopback run used Rust 1.98.0,
offline dependencies, a 16 MiB test-thread stack and one test thread; its log is
`artifacts/cli-tests-debug/provider-retries.log`. No paid provider calls were made.

The final [P2 qualification](p2-completion.md) passed all 60 native contracts,
including all three retry contracts on both stores, plus the retained
expired-header-deadline regression. Its source-hashed manifest records the final
fixture and implementation inputs.
