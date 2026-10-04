# Execution engine refinement work ledger

Approved October 4, 2026. Branch: `feature/execution-engine-refinements`. Delivery is local phase commits only; no push, PR or release.

The [architecture plan](../architecture/execution-architecture-review.md) owns implementation contracts and acceptance. [ADR-085](../adr/085-existing-execution-refinements.md) records the superseding decisions. This ledger records actual progress; prior P1/P2 completion does not qualify these changes.

## Work items

| ID | Increment | Dependencies | State |
|---|---|---|---|
| EE-00a | Evidence and quality definitions | Plan review | complete |
| EE-00b | Phase instrumentation | EE-00a | in progress |
| EE-00c | Bundle integration and reconstruction | EE-00a/b | in progress; bundle/offline reconstruction verified |
| EE-01a | Harness deadline/spend removal | EE-00a/b | planned |
| EE-01b | Versioned explicit limit representation | EE-00a | in progress; core/schema slice verified |
| EE-01c | Financial admission with durable attempt fences | EE-01b | planned |
| EE-01d | Derived bounds and start/resume conversion | EE-01b/c | planned |
| EE-01e | Settlement/retention separation and test disposition | EE-01a–d | planned |
| EE-02a | Consolidate opens and reconstruction | EE-00b | planned |
| EE-02b | Incremental validation | EE-00b | in progress; exact byte accounting verified |
| EE-02c | Separate hot state and retained history | EE-02b | in progress; shared current-read projection verified |
| EE-02d | Verified checkpoint/index hydration | EE-02c and documented integrity design | planned |
| EE-03a | Bounded output, CI fixtures and artifact access | EE-00a | in progress; normal-CI codec fixture passed |
| EE-03b | File/range working-set index | EE-03a | in progress; core reconstruction tests passed |
| EE-03c1 | Per-request allocation contract | EE-01b/c, EE-03a | planned |
| EE-03c2 | Routed allocation and encoding order | EE-03c1 | planned |
| EE-03c3 | Truncation evidence and continuation | EE-03c1; integrate c2 | planned |
| EE-03d | Conditional caching/encoding refinements | Measured EE-06 need and applicable EE-03 inputs | planned |
| EE-04a | Shared-driver completion repair | EE-00a/c; current full requirement set | planned |
| EE-04b | Focused selection and freshness refinement | EE-04a | planned |
| EE-04c | Progress pause and reason evidence | EE-04a, EE-00c | in progress; public reason projection verified |
| EE-05 | Integrated stop and resume | EE-01, EE-04; integrate adopted EE-02/03 changes | planned |
| EE-06 | Recurring execution experiment and analysis | First diagnostic slice: EE-00a–c, EE-01a, EE-03a, EE-04a; intended full collection: all EE-01 | planned |
| EE-07 | Full A/B and larger-engagement evidence review | All EE-01; scoped EE-02 acceptance; EE-03a/b/c, EE-04, EE-05 and corresponding EE-06 evidence | planned |
| EE-08 | Deferred selective constraints, if needed | Sufficient EE-07 data and separate owner decision | deferred |

## Traceability

EE-00 supports all phases with diagnostic and quality evidence. EE-01 maps to P1-05 and P2-07 plus public protocol/SDK/editor schemas. EE-02 maps to P1-04/P1-06. EE-03 maps to P2-08 and P1-05 request admission. EE-04 maps to P2-05/P2-06 and P1-05 admission. EE-05 maps to P2-07. EE-06/07 qualify the integrated behavior and do not replace independent scenario gates. EE-08 is deferred pending engagement evidence and a separate decision, as specified in the approved plan.

## EE-00a — Evidence and quality definitions

Baseline source checkpoint: `fee3a2c7`; retained local candidate: 0.2.24. The [A/B campaign record](../test-plans/ab-campaign.md) is historical evidence and retains its original financial/stop policy. It is not authority for a new enforced-mode campaign, and its stopped runs are not passes. No original data or ledger is rewritten.

Quality acceptance: full A/B authored requirements and independent gates, preservation of protected regressions, current required verification, correct stop/resume behavior, and later full engagements with explicit expected results. Record each criterion as passed, failed, blocked or untested with artifact references. A low cost or a fast run cannot compensate for incorrect or incomplete work.

Diagnostic questions: what was requested; which context and authority were current; which attempt was admitted/sent; which tool effects actually occurred; why verification/completion/refresh happened; which effects or charges remain unknown; which concrete evidence supports a proposed cause. Distinguish fact, inference and missing data.

Required joins use existing scope, command correlation, causation, attempt and artifact identities. An inspectable bundle must preserve source watermark, event ordering, join targets, missing/inaccessible evidence, check outcomes and source revisions. Successful runs and failed runs both require analysis. EE-00c fixture reconstruction validates the bundle rather than assuming logs are adequate.

The current baseline lacks complete per-phase timing, so missing spans stay unknown. Instrumented baseline comparisons retain candidate, backend, host, workspace/checkpoint, provider settings and relevant cache conditions. New measurements never overwrite prior observations.

## Phase evidence and commits

- EE-00a: evidence/quality definitions and phase registration complete. Source inspection and Markdown reference checks only; no new live scenario outcome is claimed.
- EE-00b/EE-02a first storage increment: payload-free open, replay, validation, materialization, artifact verification, append and checkpoint observations, including failed opens. Reused the already loaded replay base and verified the file checkpoint during the mandatory journal traversal, removing its duplicate prefix replay. Full historical semantic validation remains mandatory. This does not complete all phase instrumentation, resume consolidation or incremental validation.
- Storage verification: 19 unit and 4 diagnostic tests passed; conformance (6), persisted JSON (5) and replay-base (7) tests passed after the checkpoint change. The unrelated OneDrive qualification remains opt-in. Added invalid-interior/later-valid history, forged resealed checkpoint and generated differential trace regressions.
- Synthetic debug measurement, 65 commits and 3 business records: SQLite open 95.856 ms (replay 79.180 ms, full validation 60.672 ms); files open 82.567 ms (replay 74.436 ms, full validation 59.140 ms). Both validate 65 states and 2,145 event inputs. File checkpoint comparison adds no semantic prefix replay (formerly 33 extra states). This is small synthetic evidence, not A/B latency qualification; historical validation remains the measured dominant work.
- EE-00c: bundle integration underway. Reconstruction checks and actual scenario analysis remain outstanding.
- EE-01b/c core checkpoint: explicit versioned finite/unbounded limits across domain, public protocol, accounting, routing, SDK and editor usage projections. Actual costs remain finite observations. Legacy finite public-command and routing-decision hashes remain verifiable; original receipt bytes are preserved. Synthetic unbounded transitions retain reservations, attempt identity and duplicate-operation fences. CLI/lifecycle activation, editor start integration and full phase disposition remain outstanding; no paid execution qualifies this increment.
- EE-01 checks passed: domain limits (2), legacy protocol hash (1), engine public start (4), budget/accounting (10), model routing (23), escalation (12), protocol generation/provenance (9), SDK validation (9), plus the new two-backend transition and routing hash regressions. SDK and VS Code TypeScript checks passed. These checks prove the tested compatibility boundaries, not full integrated execution.
- EE-02b checkpoint: private exact serialized-state size accounting is rebuilt at reopen and updated from actual changed records, appended events/receipts and session sequences. It is installed only after successful durable append or receipt-matching replay. Public full validation and every semantic/global validator remain the reference behavior; this removes one repeated history traversal without claiming all validation is incremental. An 80-step generated trace compares exact byte lengths through record replacement/deletion, escaped payloads, sessions, receipts, rejection and duplicates. The complete store suite passed with `VCP_TEST_GIT` pointing to installed Git; existing opt-in tests remain ignored.
- Follow-up single debug sample on the same synthetic 65-commit shape: SQLite open 72.270 ms / validation 33.345 ms; files open 56.509 ms / validation 31.636 ms. These observations justify further measurement, not a stable speedup claim or full fast-state acceptance.
- EE-03a/b core checkpoint: self-contained normal-CI verbose-result fixture uses the production model request encoder with six/twelve-pair windows, Unicode/escaping, exact capacity accounting, intact call/result pairs and omitted-source tampering rejection. It does not require retained private campaign spools. Context unit tests (7) and file/range reconstruction tests (2) passed; model codec fixture (1) passed. The range index preserves multiple observations of one source, validates each unique current path once, records changed/unavailable sources, rejects cross-task history and excludes prior bindings. It emits normal send-fence probes and rebuilds deterministically from retained evidence. Lifecycle integration, scoped artifact reader integration and routed allocation qualification remain in progress.
- EE-02c checkpoint: immutable `CurrentState` snapshots omit historical events and command/transaction payloads. Repeated current readers share one `Arc` per successful commit; rejected/duplicate transactions preserve it, and older readers retain their watermark. Collection-key prefix queries avoid scanning unrelated collections. The explicitly named projection digest is separate from historical commitments. Both-backend tests pass for snapshot isolation, scope checks, bounded history-independent projection bytes and receipt preservation; diagnostics (4) and replay-base (7) tests also passed. CLI adoption and resume-open consolidation remain under integration checks.
- EE-02b integrity qualification: four adversarial event-index tests passed, but the experiment still compares the exact old event prefix and remains test-only. Automatic approval review rejected a production shortcut pending equivalence evidence. Production continues full historical event validation; no shortcut or validation waiver is delivered.
- EE-02d remains outstanding: checkpoints are compared against mandatory replay rather than trusted from their mutable local seals. Suffix-only cold open cannot yet establish equivalent detection of interior corruption. Hot snapshots do not change durable trust or qualify bounded reopen; the delivered duplicate-replay reductions and hot-read API remain useful independently.
- EE-00c reconstruction checkpoint: CLI bundle tests (8) passed, including authorization, pagination, missing evidence, duplicate identity and explicit attempt/effect associations. The offline analyzer's two tests passed and it analyzed two retained original A/B failure bundles without provider calls or evidence mutation. [Experiment notes](../test-plans/execution-engine-experiments.md) record source hashes and findings. Explicit causal-parent links were absent in those runs; shared recorded identities remain associations rather than invented causal edges. Lifecycle phase spans and a newly executed repair-chain analysis remain outstanding.
- EE-04c projection checkpoint: versioned bounded pause diagnostics are optional on public task views, retain readable reasons and link only same-task diagnostic artifact metadata. Legacy strings stay valid. Protocol decoder and engine scope/fallback tests passed (1 each), schema/provenance plus SDK tests passed (19), SDK/extension type checks passed, and extension rendering tests passed (4), including legacy fallback and text-only rendering. Controller pause/repair integration is being verified separately.
- EE-02c copy-on-write checkpoint: full state snapshots share immutable record, event and receipt collections; mutation detaches only touched components. Current snapshots also share the record map. Canonical persisted JSON and historical digests remain byte-for-byte unchanged, so this increment requires no durable migration. The complete store suite and downstream CLI current-query test passed, including both-backend snapshot isolation, replay/corruption checks and exact serialization. An opt-in debug measurement on 1,060,706 canonical bytes (3 records, 33 events, 1,000 clones) observed deep clones at 240,345 microseconds and shared clones at 408 microseconds; this is one synthetic observation, not an A/B latency result. Durable paging and bounded reopen remain outstanding.
