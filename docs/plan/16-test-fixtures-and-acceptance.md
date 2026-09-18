# 16 — Test code, fixture design and owner acceptance

Status: product test guidance planned; [delivery harness checks](../development/delivery-harness.md) exist. Product tests described below remain unimplemented. P0 establishes the harness; each owning segment adds its fixtures/tests with the behavior. Segment 15 conducts final campaigns. See [the coverage ledger](20-traceability.md).

The [qualification design](../architecture/qualification-release-design.md) specifies runner/result records, independent supervision, held-out comparisons and package evidence. Use [architecture invariants](../architecture/vcp-what.md#21-invariants-enforced-outside-the-model) as hard assertions and [ADR-018](../adr/018-release-acceptance.md) to distinguish owner acceptance from test execution. The [implementation workflow](../development/implementation-workflow.md) explains how to attach these checks to a reviewable increment.

## Test code organization

```text
src/tests/
  support/             fake clock/IDs, scripted provider, temp roots, fault barriers
  fixtures/
    repositories/      small versioned source trees and intended defects/changes
    events/ context/ memory/ search/ retention/ provider/ decision/ mcp/
  contracts/           backend, budget, provider, context, policy and service behavior
  recovery/            child-process crash/reopen and external-effect oracles
  platform/windows/    paths, process jobs, PTY, console close and filesystem behavior
  end-to-end/          compiled CLI with real store/broker and controlled providers
src/evals/
  tasks/               task manifests and private owner-fixture references
  graders/             executable artifact/check graders and human rubrics
  fixtures/            retrieval truth sets and held-out task metadata
docs/evaluations/      tracked redacted report summaries only
scripts/evals/         evaluation orchestration and result collection
scripts/test.ps1        suite/case/backend dispatcher with real exit propagation
```

Follow [the code layout](code-layout.md). These logical test directories may be attached to the retained Codex workspace/package test targets after P0 mapping. Register shared test targets explicitly in the chosen workspace; directory names alone do not make Cargo discover them. Keep pure unit tests alongside their source modules. Avoid copying the same conformance logic into each backend's test module. The shared suite instantiates both implementations and uses independently defined expected behavior.

## Harness contracts to code first

| Helper | Required behavior |
|---|---|
| Frozen clock and ID source | Stable timestamps/IDs for ordering/date tests; production crypto randomness is never replaced by this helper |
| Scripted model transport | Ordered chunks/errors/delays/usage; observable request count and reservation correlation |
| Decision response corpus | Closed answer schemas, explicit abstention and adversarial output; independently labelled routing/review cases with fixed permitted evidence |
| Fake MCP server | Versioned discovery, auth failure, schema mutation, delayed/non-idempotent effect and disconnect |
| Disposable repository builder | Named clean/dirty/staged/untracked states; reproducible base commits and expected final content |
| Fault barrier | Signal exact lifecycle boundary; supervisor kills child process there and reopens a fresh process |
| External effect oracle | Record actual marker/file/process effects independently of VCP's event store |
| Controlled sync copier | Delay/reorder/truncate/corrupt encrypted files and model offline descendants; never counts as provider certification |
| Vault observer | Watch objects during creation/error, inspect plaintext markers and verify final crypto format independently |
| Result recorder | Capture commands/versions/status/metrics with secrets excluded and durable local artifact paths |

Use fixtures with synthetic secrets and recovery keys. Real owner repositories and valid credentials stay in explicit local/private roots. Any report moved to a configured cloud vault uses the encrypted publisher. Test-only providers/fault switches cannot be silently enabled in a release configuration.

## Test layers and scheduling

1. Pure/contract tests run on relevant changes: transitions, revision/idempotency, cost arithmetic, filtering, schema conversion and manifest invariants.
2. Integration tests run real storage, CLI and broker paths with controlled model/MCP responses. Use both advertised backends and fresh processes for recovery.
3. Windows tests observe actual paths/processes/PTY/cancellation and reported isolation. Returned mock errors cannot prove OS enforcement.
4. Local inference tests provision the exact pinned model and build/query real Tantivy/DiskANN indexes on CPU. Fake embeddings cover control flow only.
5. Live OpenRouter tasks measure actual coding/routing quality under an explicit total evaluation cap, concurrency and timeout. No default network or spend in routine unit tests.
6. Release tests use installed packaged binaries, two declared Windows environments and actual cloud-folder handoff for claimed provider support.

Select tests from changed contracts and their consumers. Repeat broad campaigns after relevant changes or unresolved failures, not as ritual after every prose edit. Baseline upstream failures remain separately visible and cannot be silently treated as VCP passes.

## Proposed fixture manifest

```json
{
  "fixture_id": "portable-history-v1",
  "case": "U04",
  "seed": "fixture-seed-01",
  "backend_variants": ["sqlite", "files"],
  "provider_mode": "scripted",
  "requires": ["native-windows", "local-embedding-assets", "two-environments"],
  "fault_points": ["vault.copy.started", "restore.before_activation"],
  "assertions": [
    "canonical_records_preserved",
    "liabilities_preserved",
    "no_plaintext_in_vault",
    "wrong_key_does_not_activate"
  ]
}
```

Names are proposed test-harness contracts. Manifest loading rejects unknown assertions/cases instead of silently ignoring them. A deterministic fixture seed controls data and scheduling, never cryptographic file keys/nonces.

## U01 — Architecture-aware analysis

Fixture: a small multi-module project with documented dependency direction, nested AGENTS.md, a generated directory and an intentional boundary violation. Include similar names in a second workspace to detect memory leakage.

Run analysis in a read-only task mode through the compiled CLI; gather architecture/module explanation, file references and persisted observations. Validate cited files/revisions and the seeded boundary finding. Check no file edits, no unrelated workspace recall and appropriate inferred-versus-verified labels. A human rubric assesses whether the explanation reflects the project's actual organization.

Evidence: source/fixture revision, context manifest, references, findings, unchanged workspace fingerprint, memory records and model/cost trace. Deterministic provider fixtures verify harness behavior; live tasks verify analysis quality against predeclared thresholds.

## U02 — Code review

Fixture: a prepared diff with known correctness/maintainability defects plus benign changes and an unrelated existing user edit. Record expected findings separately from model-visible task content.

Run review with visible read-only child delegation. Check findings identify affected paths/lines and evidence, no unsolicited code writes occur, benign changes are not automatically called defects, and child progress/charges are visible. Score missed and false findings rather than exact phrasing.

Evidence: initial/final Git state, known-defect rubric, reviewer packets, event trace, executed checks and total attempt/support cost. Owner-approved quality thresholds are fixed before the held-out release run.

## U03 — Architecture-fitting generation

Fixture: a bounded feature spanning existing modules with established error handling/configuration/test conventions and a staged/unstaged/untracked user change. Supply executable feature assertions and forbidden-scope checks.

Run grouped routing with visible isolated child work, integrate through prepared edits and execute relevant tests on the final result. Assert requested behavior, preserved unrelated edits, respected module boundaries and no stale child-only verification accepted as final proof. A reviewer assesses fit with the existing project rather than a universal preferred design pattern.

Evidence: base/dirty snapshot, child patches, integrated diff, current-result verification, architecture rubric, task history/memory and root ledger including failures.

## U04 — Encrypted portable handoff

Fixture: machine A contains full transcripts, claims/disputes, vector/index generation, pending work, optimizer policy, unsettled charges and a verified recovery identity. Machine B has a different root path and separately provided recovery material. Neither has provider-managed encryption counted toward the VCP guarantee.

Perform pause, snapshot/encrypt, cloud-copy, decrypt/validate/rebind, resume, then return to A. Exercise both backends, cross-backend conversion, partial/offline hydration, incompatible indexes and two offline descendants. Run all crypto/key/archive/failure cases from segment 11.

Assert complete retained logical records, preserved liabilities, no automatic new-host grants, no plaintext payload/metadata/key in observed vault writes and no activation with wrong key, untrusted writer, corruption or missing data. Keep last valid state on failed activation; enforce deletion epochs on restore.

Evidence: canonical before/after comparison, encrypted object inventory, observer log, independent decrypt/authentication results, key-reference-only status, sync-provider/Windows details and actual restored search/task readiness.

## U05 — Full history and pruning

Fixture: timestamps just before/at/after the 30-day boundary; full oversized tool and child output; inferred/disputed/superseded claims; active recovery and unsettled cost dependencies; older retained snapshots.

Browse/filter by date/path/model/agent, inspect full content beyond UI tails, preview selections, mutate state, then test exact apply or stale-preview rejection. Exercise exclude/compact/purge separately and kill during cleanup. Default notices must not delete records.

Assert selected IDs/timezone semantics, protected references, blocked stale-context dispatch, no recalled purged content and explicit backup retention limitations. Evidence distinguishes tombstoning, local physical cleanup and remaining cloud copies.

## U06 — Close, kill and resume

Fixture: active root model stream, read child and a child command that writes an independent non-idempotent marker. Place barriers around reservation, dispatch, actual effect and receipt.

Exercise explicit pause, Ctrl+C policy, actual console close, lost controlling pipe and forced process termination. Reopen the workspace in a new process. Assert no new unattended scheduling, bounded owned-process cancellation, preserved partial artifacts/child graph, no duplicate marker and correct unknown liability/effect state.

Changed files, expired grants, missing worktree and moved roots must require appropriate revalidation. Test PID reuse/ownership checks before termination. Record genuinely unkillable/unobservable external effects as unknown rather than claiming they were undone.

Explicit pause without closing is a required separate variant. Keep the owning CLI alive, submit `/pause`, observe the stop boundary and durable paused state, inspect history/cost/children, then `/resume` in that same process after current-state checks. Repeat pause while already paused and record steering while paused; neither may issue new model/tool calls. Independently paused children stay paused when the parent resumes. See the [same-process experiment](../architecture/qualification-release-design.md#integrated-pause-experiment).

## U07 — Routing and optimization

Fixture: simple and difficult tasks plus complete, sparse, biased and pruned history windows. Pin candidate metadata and recorded quality evidence; include denied providers, unknown prices and strict model pins.

Verify group/profile distinction, exclusions, total-cost reservation, bounded escalation and compatible handoff. Run `/optimize`, answer/decline its questions, inspect/apply selected policy differences and roll back. Assert no silent pruning, authority change, cap increase or paid trial.

Live comparisons use matched task/configuration conditions and held-out examples. Record failures, support/child/compaction cost and uncertainty; report correlation when comparisons are not controlled.

Run the same scenarios with optional semantic evaluation disabled, shadowed and
advisory, following [ADR-020](../adr/020-bounded-semantic-decisions.md) and
[decision qualification](../architecture/decision-evaluation-design.md#qualification-and-rollout).
Disabled mode must perform no evaluator network calls. Unavailable, abstaining or
rejected output uses the deterministic baseline unless an enabled policy permits
a separately qualified conventional evaluator fallback; required review remains
intact in every mode. Shadow mode records advice without affecting actions but
still requires scoped context, admitted spend and visible usage. Use scripted transport for routine contracts;
live comparisons need separately authorized caps and never run in default CI.

Cover three distinct implementations: deterministic rules, actual Jev through
OpenRouter as the preferred specialized candidate, and a conventional OpenRouter
LLM comparator/permitted fallback. A stronger conventional LLM comparison is
optional. First qualify the actual Jev endpoint, question/schema capabilities,
native answer fields and usage mapping using
[the gateway qualification contract](../architecture/decision-evaluation-design.md#jev-through-openrouter-qualification).
The `typesafe/jev-1.13` planning example is not an immutable promise: record exact
requested/served identities, observation dates and immutable versions where
available, refreshing availability and prices for the run. If actual Jev cannot
be tested, record not run; conventional-LLM emulation cannot stand in as its result.

Include malformed Boolean/Choice/Score responses, duplicate/missing question IDs,
out-of-range/non-finite scores, invalid probability distributions, inaccessible
evidence and advice naming an ineligible model. Test timeout, repair exhaustion,
zero balance, concurrent reservations, pause before send and new steering before
response application. Join each observed evaluator request to its reservation,
including retries and shadows; uncertain usage survives reopen. Assert no recursive
evaluator selection or schema-repair loop, no authority expansion and no skipped
required review/test on a low risk score. Replaying persisted advice does not send
another request; changed revisions require revalidation, not silent reuse.

Native response fixtures distinguish Boolean yes-probability from a Boolean
decision: no implicit fixed threshold turns the former into the latter. Verify
versioned Rust thresholds and abstention. Choice/Score confidence fields, choice
distributions and numerical scores retain their separate meanings; neither a
winning-choice probability nor a provider confidence field proves calibration.
Missing native probabilities must fail the affected probability mode or use a
separately qualified discrete-only mode without manufacturing certainty.

Exercise Jev outage, renamed/changed aliases, unknown served versions, schema
drift and narrowed provider data controls. Conventional fallback must be explicitly
permitted and separately qualified, use current scoped context and fresh admission,
retain Jev's uncertain charges and share the original deadline/attempt/budget
limits. Assert no direct TypeSafe endpoint, alternate credentials or hidden retries.
Use hostile state strings containing fake questions, options and instructions.
Exact counts, date ordering, arithmetic and scope/authority checks have independent
Rust truth fixtures and cannot be delegated to or overruled by the classifier.

For E19/U07 quality comparisons, declare tuning, calibration and held-out splits,
truth labels and per-purpose thresholds before measurement. Include serious
seeded defects and benign changes, productive repeated attempts and true stalls.
Record abstention coverage, false/missed escalation and review, end-to-end quality
and total successful/failed-task cost including evaluator repairs and shadow
overhead. A Score is not calibrated confidence; probability calibration and Brier
scores apply only to explicitly probabilistic outputs with suitable labelled
evidence. Keep p50/p95 latency, sample limits and model/prompt drift visible.
Unqualified advice stays disabled and cannot lower the existing quality floor.
Separate actual Jev, conventional fallback and deterministic result cohorts; do not
credit a fallback's outcome or cost to Jev. Include per-mode probability availability
and semantic mapping failures in qualification evidence and rollback triggers.

P8 repeats enabled-purpose scenarios with real child admission/isolation,
integration and independent review, including U02/U03/U06. Child-event scripts in
P6 do not establish those outcomes. Local retrieval/memory correctness remains
covered by M/U09 suites with the evaluator absent; a remote decision service cannot
become a hidden prerequisite or embedding fallback. P10 observer agents remain
deferred.

## U08 — Skills and MCP

Fixture: the declared built-in language/toolset coverage matrix, nested AGENTS.md, available/missing Windows tools, and controlled MCP servers with colliding names, changing schemas and delayed effects.

Assert lazy discovery/activation, visible skill provenance, project-convention fit, correct missing-tool diagnosis and no unauthorized toolchain installation. MCP calls require current server/schema identity, policy and durable intent; disconnect/cancel cannot cause blind non-idempotent replay.

Use actual execution only where the tested Windows host supports the toolchain. Analysis/review/generation guidance for an unavailable platform is a separate capability from successful compilation. Hooks/importers are not prerequisites.

## U09 — Local memory compute

Fixture: a pinned local embedding model/runtime and retrieval corpus on a CPU-only Windows environment. Disable embedding-network access and run code/index/query without hosted VCP services.

Exercise model provisioning, digest mismatch/missing assets, batching limits, index build/reopen and offline recall. Compare ANN to exact vector search and report lexical/vector/fused quality. Track embedding time, query latency, resident/mapped memory, build peaks and disk growth.

Assert no remote embedding request, truthful setup/degraded states and usable local recall from qualified assets. Do not infer minimum hardware or large-corpus performance from a tiny fixture.

## Fault points and independent assertions

Minimum barriers: artifact finalize; canonical commit before/after acknowledgement; reservation before/after send; authorized intent before/after dispatch; effect before outcome commit; each index component/manifest/pointer; prune tombstone/payload cleanup; encryption finalize/vault copy; restore validate/activate; child integration/result commit.

For each barrier record observed effects independently, kill the process, reopen and compare the correct expected acknowledgement envelope. Distinguish process-kill guarantees from power-loss/media-loss claims. Never “fix” a recovery test by resetting the data root it is meant to validate.

## Metrics and pass records

Report pass/fail/not-run per case; fixture/model/provider/backend/version; current source/dirty state; known/unknown money; time distribution; missed/false findings; unauthorized/duplicate effects; lost acknowledged records; stale/restricted recall; and output/capture gaps. Failures remain in denominators.

Provisional architecture performance numbers are targets, not results. Specify numeric quality/cost/latency thresholds and corpus/task sizes before final evaluation; user safety/correctness invariants are hard gates. A suite with a missing Windows environment, key, model asset or toolchain cannot report success for that case.

## Implementing the suite registry and fixture protocol

The registry maps each case to real test targets, supported backend variants, required resources and result schema. Validate selection before launching anything. `-Backend both` expands to independent sqlite/files executions plus explicit conversion cases where applicable; it must not run one implementation and label the result parity. Unknown case IDs and unsupported suite/backend combinations fail with diagnostics.

Use a harness-owned control channel for barrier readiness and a separate channel for product output. Bound both, and include run/case/attempt/barrier IDs in control messages so stale signals cannot release a later attempt. The supervisor owns timeouts and process termination; the engine cannot declare that its own crash test passed.

Fixture setup records exact expected base files, Git index/working/untracked state, canonical seeds, truth-set IDs and synthetic secret categories. Apply permissions and size limits before launching product code. Teardown checks the recorded resolved resource root and retains failure evidence. Never run a broad cleanup over user data.

## Independent oracles by boundary

| Boundary | Observe independently | Do not accept as sole proof |
|---|---|---|
| Budgeted request | Scripted transport request count joined to admitted attempt/reservation IDs | Router says it requested a reservation |
| Advisory decision | Closed-set truth labels, eligible candidates, actual selected action and request/ledger correlation | Evaluator confidence, agreement with itself or schema validity alone |
| Prepared edit | Initial/final bytes, staging state and outside-scope sentinel files | Tool reports success |
| Pause/recovery | Supervisor barrier, actual marker effects and child process tree | Task projection says paused |
| Canonical durability | Acknowledgement log plus fresh-process export/reference checks | In-process cache returns the record |
| Retrieval | Separate authorized relevant-ID set and forbidden text sentinels | Search returned some neighbors |
| Encryption | Intermediate vault observation plus independent decrypt/authenticate/tamper cases | Filename extension or plaintext-marker absence |
| Portability | Before/after neutral records, liabilities and deletion lineage | Restore command exits zero |
| Distribution | Clean-profile install of exact final artifact digest | Developer checkout compiles |

Production assertions can aid diagnosis, but the oracle must not merely call the same function used to produce the expected value. For arithmetic, use small manually derived boundary cases alongside generated tests. For ANN, exhaustive distance comparison uses the exact same saved vectors and declared metric, not a separately re-embedded corpus.

## Acceptance scoring construction

| Case | Primary objective assertion | Supplementary quality assessment |
|---|---|---|
| U01 | Correct cited source revisions, seeded boundary finding, unchanged workspace | Explanation reflects actual module direction; unsupported claims counted |
| U02 | Seeded defect matches and benign-change false positives, no writes | Finding usefulness/severity and evidence sufficiency |
| U03 | Executable requested behavior, preserved user edits, verified integrated state | Fit with existing errors/configuration/module conventions |
| U04 | Complete canonical/reference/ledger comparison and rejected unsafe restore | Setup/handoff clarity and measured time/resource envelope |
| U05 | Exact selector/protected sets, default no-delete and no stale recall | Ability to explain cleanup and retained-copy limitations |
| U06 | No new dispatch after stop, no duplicate effects, same-process pause works | Visible progress/reconciliation and deliberate continuation |
| U07 | Eligible reproducible decisions, bounded cost, exact chosen policy/rollback | Matched total task quality/cost/latency versus fixed baselines |
| U08 | Correct identity/schema/authority and no unsolicited installation | Useful ecosystem guidance under declared host constraints |
| U09 | Real offline CPU embeddings, persisted indexes and authorized recall | Query/build resource distributions on declared corpus sizes |

Set numeric quality thresholds and sample sizes before the final held-out runs; invariants use zero forbidden outcomes in the declared suite. Record abstentions, not-run checks, failures and intervention instead of scoring only final successful responses. Keep owner review, executable checks and optional model grades in separate fields. A model grader itself needs admitted spend and cannot overrule a failing executable or authority assertion.

## Reproducibility and evidence invalidation

Each result binds source commit and dirty diff, fixture/truth-set revision, backend/configuration, selected model/provider/catalog, local asset specification and package digest when used. Record seeds for fixture construction and repeated scheduling; do not claim live model outputs are deterministic because the harness seed is fixed.

Changing a contract or its implementation invalidates affected consumer tests. Changing only a renderer need not rerun local embedding quality unless passage selection changed. A changed model, tokenizer, metric or corpus invalidates retrieval comparison; a changed package/runtime dependency invalidates the relevant installation/native checks. Keep an explicit reason for the selected rerun set.

Reports retain original failed attempts and link follow-up runs. A required not-run case blocks qualification even if the available subset is green. Documentation link/ledger validation proves plan consistency only and must never be reported as passing the runtime suites defined here.
