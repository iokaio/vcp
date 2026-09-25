# P10-03 optional observer evidence

The selected [observer](../development/observers.md) is the opt-in
`exact-verification-repetition/1` root diagnostic, under
[ADR-067](../adr/067-bounded-local-observation-tasks.md). The
[grading plan](p10-03-observer-grading.md) was declared before qualification.

## Reproduction and qualification

Run `./scripts/test-observers.ps1` on native Windows with the pinned Rust 1.98.0
toolchain, real Node/Git/Cargo executables and Visual C++ x64 tools. The final run
is recorded in
`artifacts/observers/bd691efb-38f3-4cd3-bbe4-580a20a08d66/manifest.json`.
It binds each input and stage log by SHA-256 and rejects source changes during
execution. Its Git revision identifies the base commit; input hashes identify
the implemented working tree tested before commit.

| Boundary | Passing tests |
| --- | ---: |
| Pure observer state, deduplication and budgets | 8 |
| Observer retention on both stores | 2 |
| Existing receipt/advisory redaction | 3 |
| Streaming source byte/deadline bounds | 2 |
| Matched pure campaign (12 labeled fixtures) | 1 |
| CLI unit contracts | 101 |
| Paused native terminal and unchanged liabilities | 1 |
| Native observer lifecycle on both stores | 10 |
| Existing native verification regressions | 6 |

The observer kill helper is marked ignored for direct invocation and is explicitly
launched by its passing supervisor. Existing opt-in cloud-path and over-120-second
verification cases remain outside this suite; the existing inherited-handle
subprocess helper remains indirectly exercised. No test was weakened to obtain
these results. Development failures corrected runner compiler setup and fixture
shutdown/display handling before the final run.

Repository delivery checks passed all 18 cases in
`artifacts/p10-observers-fast-final/13554dc6-4e20-4054-aea7-f43f55cbc1d9/manifest.json`.
Scoped Rust formatting and diff checks pass. Retained Codex source and indexed
modes/bytes remain unchanged: 7,940 files, inventory digest
`ec5b348ce6ce9db6d181810db490c5fdadebdd77ddb7e9a2fe9963464a470275`.
No dependency or public wire schema changed.

## Measured diagnostic result

All 12 declared pure fixtures matched their labels: four exact-repetition
positives and eight abstentions. The positive patterns group three or six
verification references into one notice. Pure analysis measured 0–519 microseconds
per fixture; negative fixtures still incurred local analysis and produced no
notice. Positive serialized proposals occupied 593–827 bytes. These small fixture
timings are observations, not performance guarantees.

The native matched fixture executes three actual failed Node checks in each arm:

| Store | Enabled | Notices | Local evaluations | Complete poll | Retained observer bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| SQLite | No | 0 | 0 | 0 ms | 0 |
| SQLite | Yes | 1 | 1 | 166 ms | 1,888 |
| Files | No | 0 | 0 | 0 ms | 0 |
| Files | Yes | 1 | 1 | 171 ms | 1,888 |

Every arm made zero main/helper requests, charged zero billable micros, retained
zero provider liability and made zero controller interventions. Each enabled arm
reserved 4,096 local steps for one evaluation; eight duplicate polls did not
reserve another evaluation or duplicate its notice. Preparation/debounce/compute
reported 56 ms on SQLite and 60 ms on Files, a subset of the complete poll above.
Zero milliseconds means below timer resolution. Startup and the three checks
are outside the timed observation poll, identically in both arms.

## Interpretation

Exact repeated checks are facts about retained evidence, not inferred stalls.
The fixture oracle includes productive and environment repetitions as factual
positives, and rejects incomplete patterns, changed diagnostics, success resets,
unavailable checks, source edits, steering changes, gaps and pruned evidence.

Grouped references reduce the number of individual records needed to inspect
the fixture's repeated pattern. This is a navigation proxy, not measured human
time or task-success improvement. Existing explicit cycle inspection can retrieve
the same grouping without enabling the observer. The added capability is bounded,
durable automatic observation at owner safe points, with visible advice.

The campaign does not establish an advantage over the best manual workflow or
justify default enablement. Negative traces incur local computation without a
notice. The observer remains disabled by default. M10 statistical regime
inference remains unqualified and is not enabled by this increment.

## Safety and recovery boundaries

Native cases use actual Node acceptance checks on unchanged files through
`host.verify`, on both Files and SQLite stores. Fresh verification now preserves
the task revision when its fingerprint is unchanged; changed input still advances
it. This prevents no-op revision churn from making real repeated checks invisible.

Qualification covers bounded storms and coalescing, duplicate inputs and facts,
local attempt/step limits, exhausted shared monetary caps, automatic verification
safe points, observer-owned event backlogs, pause, correction, expiry, reopen,
and forced native owner-process termination. Interrupted work is not replayed.
The slow-result cases use a controlled local completion barrier; the selected
observer has no provider path to delay or replay.

Forgetting contributing verification evidence suppresses advice immediately and
redacts its durable projection. A content-free tombstone prevents budget reset.
The terminal case pauses an actual in-flight main request, requires unresolved
liability, then checks that `/observers` leaves ledger, attempt and reservation
records unchanged and creates no additional request.

## Measurement limits

Local fixtures make no billable main or observer requests. Their zero-dollar
result does not predict production task costs. Local elapsed time is wall time,
not CPU or peak-memory utilization. Status measurements exclude final publication;
the native campaign additionally times the complete poll. Configuration startup
and human inspection time are not included in that poll measurement.

Qualification covers native Windows process loss, not machine power failure or
another operating system. A conservative source-size ceiling can cause visible
abstention on larger histories. Root-only exact verification observation is the
qualified scope; recall/goal observers, model observers, default enablement and
P10-04 require separate evidence.
