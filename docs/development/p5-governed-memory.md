# P5-01 governed memory

`vcp-memory` commits typed proposals through the canonical Files or SQLite store.
Registry version 1 covers test/build command observations, module relationships,
architecture decisions, environment constraints, verified fixes and explicit user
preferences. Claim resolution and evidence status are separate: automatic
acceptance does not turn an inference into a verified fact. Remembered commands
remain data and confer no execution authority.

The adapter calls the retained Munarium `run_gates` implementation from revision
`8da666067000ca1ee9c131bc67e70b978862faa3`. It supplies an authorized immutable
snapshot; no Munarium database, server or model client participates. VCP adds
workspace/task scope, versioned structured values, retained artifact integrity,
current verification, predecessor and source applicability checks. Typed value
digests preserve case-sensitive arguments and symbols instead of adopting
upstream free-text case folding. Separate source fingerprints remain separate
observations. Inferred architecture stays inferred. Overlapping contradictions
are disputed, not resolved by time or confidence; a correction requires the
current predecessor, actor and reason. Capacity failures are explicit.

One transaction retains the proposal, resolution, immutable version (when
accepted or disputed), head, memory sequence, event, pending index intent and
idempotent result. Rejected proposals retain findings without turning alleged
foreign evidence into authoritative links. The origin/extractor/output identity
also detects retries. An identical retry returns the original receipt, including
after reopening; changed content under that identity fails. Index publication is
a separate operation and the receipt reports pending indexing.

Historical queries select immutable versions first, then apply current authority,
task access and retention. A removed origin or evidence returns a pruned row
without claim text. Retained unavailable evidence has an explicit availability
label. A changed source fingerprint or stale check invalidates applicability,
without rewriting the historical observation. Head and sequence projections can
be rebuilt from immutable records; rejected proposals still consume sequence
numbers and retry results are never dropped as disposable projections.

Command outcome proof is bound to the native retained `verification-check/1`
plan and configuration digest, rather than merely borrowing a passing check.
Native verification currently records test runners; a build observation can be
retained without claiming a verified outcome. A verified fix requires a
`vcp-memory-change/1` artifact containing exact `base` and `current` source
manifests, whose canonical hashes match its before/after repository fingerprints,
plus current successful verification of the after fingerprint. This establishes
the observed source delta and checked result, not causal authorship or proof of
the proposed diagnosis.

The bounded adapter accepts proposals up to 256 KiB, returns at most 256 versions
per history query, and retries a canonical watermark conflict at most three
times. Source proof JSON is bounded before reading. These are implementation
limits, not measured indexing or retrieval throughput guarantees. Ingestion,
local search publication, pruning commands and CLI memory surfaces belong to the
following dependent work items.

## Qualification

P1-04 provides the canonical transaction/artifact contract; P0-02/P0-07 provide
the selected Munarium source, license inventory and offline dependency closure.
The workspace membership is recorded in Codex patch 0025 and its exact source
inventory. Native Windows tests exercise the same governed fixtures on Files and
SQLite, including immutable store contracts and actual retained upstream gates.

Native Windows, stable Rust, offline locked dependencies, 2026-09-19:

```text
cargo +stable test --locked --offline -j4 -p vcp-memory -p vcp-domain -p vcp-protocol -p vcp-store -p vcp-audit --tests
cargo +stable build --locked --offline -j4 -p vcp-memory
cargo +stable clippy --locked --offline -j4 -p vcp-memory -p vcp-store -p vcp-domain -p vcp-audit --tests --no-deps
scripts/test.ps1 -Suite fast
```

All 52 Rust tests passed: memory 18, domain 9, protocol 2, store 16 and audit 7.
The ten durable memory cases exercise both storage backends. They cover lost
acknowledgement/reopen, payload and origin collisions, missing/foreign evidence,
contradictions, supersession, current authority and narrowed origin-task access,
retention after reopen, all six classes, unrelated command/patch proof, stale
verification and projection repair including rejected sequence advancement.
Audit regression coverage also proves generic history and inspection cannot
expose derived text outside the governed query boundary.

The production library build, formatting, diff checks and all eight fast harness
cases passed. Clippy passed with existing domain/store warnings; the new memory
crate emitted none. Codex inventory/boundary verification and exact forward and
reverse application of patch 0025 passed. Full replay from the original upstream
checkout was not run because that checkout is unavailable locally.

Ignored logs are `artifacts/p5-01-final-tests.log`, `p5-01-build.log`,
`p5-01-clippy.log`, and `p5-01-fast.log`. The fast harness initially encountered
the sandbox's different Windows Git owner; running it with the host identity
resolved that environment issue without changing repository trust configuration.
