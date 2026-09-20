# P5-02 activity and evidence ingestion

The canonical host advances durable ingestion at task, completed-turn and check
boundaries; capture and tool hot paths retain raw observations for the next step.
Each step
scans at most 32 events, creates at most 16 jobs within 256 KiB, and processes at
most two jobs before returning control. The workspace pending quota is 256;
concurrency is one, leases last 30 seconds, and a job has at most three attempts.
A deferred failure waits at least one second. Explicit `maintain_memory` runs
the same bounded step. Closing the owner, pausing a task or its ancestor, or
fencing canonical capture prevents new task-scoped work. Inspection never calls
this maintenance entry point.

A cursor is keyed by root scope and extractor specification. Advancing it and
creating pending jobs is one canonical transaction. The store checks that every
selected scanned origin has its uniquely identified job, and validates cursor
monotonicity, immutable provenance, leases, result links and terminal transitions.
A lease can expire; correctness comes from durable output identities and P5-01
receipts. Retrying a partial extraction includes already committed outputs even
if today's source availability changes. A stale authority or retention decision
fails visibly rather than regenerating a different proposal under the same ID.

`deterministic/1` reads authorized retained artifacts with a cumulative 256 KiB
input budget, at most 64 artifact references and 16 proposals. It extracts
configured native test commands and recorded outcomes from exact verification
receipts. `memory-observations/1` carries explicit typed source/module,
architecture, environment, command, fix and correction observations. Unsupported
raw prose stays activity history; it does not become an invented fact. Child,
completed/interrupted work and source changes retain explicit findings. An
unobserved editor or external process remains an unknown actor.

Explicit user preferences have the objective form
`{"memory_preference":{"key":"style","value":"concise"}}`. The complete
objective must be this bounded JSON value. The host captures it as evidence and
binds the exact key/value to the canonical root task creation or steering event.
Child objectives and child steering are excluded: child commands can share the
host actor, and current events have no direct-user provenance marker. Such input
remains task activity, including when it has the typed preference shape. A model or another
artifact cannot invent a preference by citing an unrelated user event. Ordinary
task prose is not parsed as an implicit global preference.

The optional model path uses an already captured gateway response. The
`promote_memory_response` host entry point requires a live root, current task
steering, a Memory-role attempt, its reservation/root ledger, and a matching
applied final-usage settlement linked to the retained response. It uses the
existing provider stream parser and requires one completed assistant message
without tool calls. There is no memory-specific provider client or repair call.
Any new extraction call or repair must pass normal gateway budget admission.
Uncertain outcomes retain their ordinary accounting liability.

Candidate JSON cannot choose actor, workspace, policy, predecessor, global scope
or new evidence IDs. Host inputs supply applicability and an authorized evidence
set. The parser bounds bytes, depth and candidate count, rejects unsupported
fields and identities, and preserves model provenance as inference. Explicit
preferences remain the deterministic user-origin path. Candidates still pass
the same governed repository; a successful extraction is not acceptance.

Capture exclusions for authentication headers and recovery material are
structural secret boundaries. They do not imply missing evidence bytes.
Unobserved tails, capture failures and explicit aborts remain incomplete evidence.

Memory inspectors expose cursor progress, queue state, attempts, leases,
completion references and bounded structured findings. Truncation is explicit.
`memory_status` reports pending/leased/deferred/failed/completed/cancelled counts,
the oldest pending watermark and accepted/disputed/rejected output counts.
Both interfaces are read-only and continue to apply current source retention.

## Qualification

Native Windows, stable Rust, Visual C++ x64 environment and locked offline
dependencies, 2026-09-19. `CARGO_TARGET_DIR` is the ignored
`artifacts/codex-target`. Native tests set explicit Git, Node and real Cargo
executables plus the compiler PATH/INCLUDE/LIB/LIBPATH, as in
`scripts/test-integration.ps1`.

```text
cargo +stable test --locked --offline -j4 -p vcp-memory -p vcp-domain -p vcp-store -p vcp-audit --tests
cargo +stable test --locked --offline -j4 -p vcp-lifecycle --test canonical_host -- --test-threads=1 --nocapture
cargo +stable clippy --locked --offline -j4 -p vcp-memory -p vcp-store -p vcp-domain -p vcp-audit -p vcp-lifecycle --tests --no-deps
cargo +stable build --locked --offline -j4 -p vcp-cli --bin vcp
scripts/test.ps1 -Suite fast
```

The final core run passed 75 tests: memory 41, domain 9, store 18 and audit 7. Fixtures
cover both-backend queue/reopen/retry, expired/stale leases, bounded terminal
failure, held-child fairness, partial-output recovery, actual native check
receipts, exact preference capture and sealed-orphan recovery, accounted model
responses, prompt-injection/foreign-evidence rejection, retained uncertainty,
and all three incomplete-content omission classes. Production CLI build and
Clippy passed; warnings remain in pre-existing domain/store/lifecycle code.
The memory crate emitted no warning. All eight fast harness cases passed.
After tightening preference provenance, all eight extractor tests, ten governed
memory tests and five runner tests passed, including the new same-actor child
creation/steering regression on both backends.

The process-recovery fixture passed all six actual abnormal-termination barriers:
queued jobs, a committed proposal before job completion, and completed jobs, on
files and SQLite. It compares the entire committed state after reopen, accounts
for every selected origin, and checks immutable results, exact retry receipts,
single versions/index intents, expired leases and idle no-op behavior. This
fixture exposed a misleading `caught_up` flag when processing emitted more
activity; the runner now includes newly emitted activity in that flag, with a
focused both-backend regression. The recovery target has two test entries (one
child entry point and one parent exercising the six kills).

All 26 native host tests passed in 684.11 seconds. Its new
both-backend fixture checks automatic maintenance, a maximum of two jobs per
step, unchanged paused/read-only state, explicit unknown edit attribution,
backlog draining and reopening. Initial broad attempts omitted the fixture's
explicit compiler setup or serial execution; those attempts are not counted as
qualification. Isolation also found an older test incorrectly expecting the
retained lifecycle's local-hold API to validate workspace authority. The corrected
regression clears that local hold and proves the canonical task remains paused
and rejects another retained model turn after trust revocation.

Exact Codex inventory/boundary checks and forward/reverse byte replay of patch
0026 passed. Logs are `artifacts/p5-02-tests-final.log`, `p5-02-lifecycle-final.log`,
`p5-02-ingestion-recovery.log`, `p5-02-host-memory-final.log`,
`p5-02-clippy-final.log`, `p5-02-build-final.log` and `p5-02-fast-final.log`. The limits above
are explicit scheduling bounds, not release throughput guarantees.
