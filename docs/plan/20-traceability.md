# 20 — Work-item, requirement and test traceability

Status: implementation and qualification in progress. Ownership inventory from architecture draft 0.4 and the task headings in this directory, with developer-workflow evidence aligned on September 22, 2026. All 68 architecture items have exactly one implementation owner: 56 first-release items and 12 deferred items (P4, P9 and P10). The evidence-linked rows below record completed P0–P3, P5, P6 and P7 boundaries. P7-04/05/06 completion includes the passing final source-bound native gate; P8 is now closed by owner direction with qualification gaps retained; P9 and P4-01 through P4-05 are complete. Task dependency IDs remain unchanged.

Use this ledger with [the segment index](README.md) and [the shared test guide](16-test-fixtures-and-acceptance.md). Dependencies below retain the architecture's exact IDs; slash suffixes share the preceding phase, and an ellipsis denotes an inclusive range. A whole segment is not an additional dependency.

The current [P7-02 completion qualification](../evaluations/p7-02-completion.md)
records corrected trial bounds, tool guidance and additional debug/data controls.
Its selected live acceptance is complete. The [P7 delegation follow-up](../evaluations/p7-interruption-exploration-2026-09-22.md)
records the passing final source-bound native gate and completed P7-04/05/06
acceptance, preserving the earlier failed runs and separate focused repairs.

## Work-item ownership and readiness

P8-01 through P8-05 are complete by [owner-directed closure](../adr/042-owner-directed-p8-closure.md), not by full
qualification. Their open-evidence descriptions remain factual limitations.
P9-01, P9-02 and P9-03 are complete. P4-01 through P4-05 are complete; packaging qualification retains its documented platform and release limits.

The [production qualification supplement](../evaluations/p8-production-qualification-2026-09-22.md)
adds an exact optimized artifact, bounded startup observations, selected package
checks and a frozen owner-fixture proposal to the P8 evidence below. That
historical evidence did not close P8; the later owner decision above does.

Each linked task supplies code organization, implementation increments and testing instructions. Evidence IDs refer to current architecture/workbook cases with the release scoping below. Update status with actual evidence links during implementation; P0/P1 evidence qualifies their stated boundaries while full product acceptance remains outstanding.

| Work item | Required dependencies | Detailed owner | Architecture acceptance evidence | State |
|---|---|---|---|---|
| P0-01 Requirements and ADRs | None | [Implementation and tests](01-upstream-feasibility.md#p0-01--requirements-and-experiment-harness) | CLI/Windows priority, Apache-2.0, local embeddings, plaintext active files, encrypted cloud backups and complete usable milestone preserved | complete |
| P0-07 Upstream pins | P0-01 | [Implementation and tests](01-upstream-feasibility.md#p0-07--immutable-upstream-selection) | Immutable manifest, R08; [consolidated selection acceptance and final green CI](../evaluations/p0-07-selection-gate.md), [Codex import](../evaluations/p0-07-codex-import.md), [native datastore](../evaluations/p0-07-munarium-datastore.md), [shared Munarium import](../evaluations/p0-07-munarium-import.md), [Gemini boundaries](../evaluations/p0-07-gemini-baseline.md), [local CPU embeddings](../evaluations/p0-07-local-embeddings.md) and [helper traces](../evaluations/p0-07-helper-traces.md) | complete |
| P0-02 Local memory/runtime spike | P0-07 | [Implementation and tests](01-upstream-feasibility.md#p0-02--local-munarium-and-search-spike) | [Real corpus/index evidence](../evaluations/p0-02-local-corpus.md), [governance qualification](../evaluations/p0-02-local-governance.md); [offline CPU evidence](../evaluations/p0-02-offline-embeddings.md), [three-scale resource evidence](../evaluations/p0-02-local-resources.md); bounded U09 prototype only, durable/product integration remains P0-04/P1/P5 | complete |
| P0-03 Internal lifecycle seam | P0-07 | [Implementation and tests](01-upstream-feasibility.md#p0-03--codex-lifecycle-seam) | [Continuation admission regressions](../evaluations/p0-03-continuation-admission.md), [scoped host qualification](../evaluations/p0-03-scoped-lifecycle.md); [durable recovery, startup/effect fencing, private CLI pause and process quiescence](../evaluations/p0-03-recovery-execution.md); production integration remains P0-08/P1/P2/P3 | complete |
| P0-04 Portable storage comparison | P0-01/02 | [Implementation and tests](01-upstream-feasibility.md#p0-04--storage-and-encrypted-portability-comparison) | [M01/M02/M08/U04 prototype evidence](../evaluations/p0-04-portable-storage.md): backend parity/crash/conversion, signed age snapshots, independent interoperability and encrypted transfer measurements; [second-Windows qualification passed](https://github.com/iokaio/vcp/actions/runs/35364917717); production formats remain P1/P5 | complete |
| P0-05 Windows execution spike | P0-03/07 | [Implementation and tests](01-upstream-feasibility.md#p0-05--windows-execution-spike) | [E07/E08/R04 native prototype evidence](../evaluations/p0-03-recovery-execution.md): Job Object tree termination, argv/output/paths and independent AppContainer canaries; general tool sandbox remains P2/P8 | complete |
| P0-08 Codex integration baseline | P0-02/03/05/07 | [Implementation and tests](01-upstream-feasibility.md#p0-08--codex-integration-baseline) | [Native integration and retained regressions](../evaluations/p0-08-09-integration.md), [retained module map](../development/p0-handoff.md), exact reconstruction and representative fix-import rehearsal; bounded R02–R05/R08, production integration remains P1–P8 | complete |
| P0-09 Gemini port fixtures | P0-03/07 | [Implementation and tests](01-upstream-feasibility.md#p0-09--gemini-fixture-and-port-boundaries) | [Pinned source comparison and attributed neutral Rust ports](../evaluations/p0-08-09-integration.md#gemini-comparison); argument rewrites, stale approval, out-of-order results, resource conflicts and cancellation; bounded R02/R03/R05 | complete |
| P0-06 Baseline qualification | P0-02…05, P0-08/09 | [Implementation and tests](01-upstream-feasibility.md#p0-06--qualification-and-handoff) | [Consolidated prerequisite evidence and candidate decisions](../evaluations/p0-06-handoff.md), [setup and concrete P1 source map](../development/p0-handoff.md); common Rust 1.98 CLI build, native regressions and source checks passed; bounded feasibility only | complete |
| P1-01 Domain state | P0-06 | [Implementation and tests](02-engine-state-and-capture.md#p1-01--domain-state) | [Foundation](../development/p1-foundation.md) and [native acceptance](../evaluations/p1-completion.md): generated transitions, current verification, stable rebinding and retained task/native-effect consumers | complete |
| P1-02 Internal commands/events | P1-01, P0-03 | [Implementation and tests](02-engine-state-and-capture.md#p1-02--internal-commands-and-events) | [Foundation](../development/p1-foundation.md) and [native acceptance](../evaluations/p1-completion.md): durable receipts, authenticated retries, JSONL parity, bounded subscriptions and retained command consumers | complete |
| P1-03 Full capture/artifacts | P1-01 | [Implementation and tests](02-engine-state-and-capture.md#p1-03--full-capture-and-artifact-staging) | [Retained host](../development/p1-retained-host.md) and [native acceptance](../evaluations/p1-completion.md): full binary/request/response/output capture, exact prefix recovery, credential separation and real capacity-failure fencing | complete |
| P1-04 Canonical backends | P1-01/03, P0-04/06 | [Implementation and tests](03-storage-and-budget.md#p1-04--storage-increments) | [Foundation](../development/p1-foundation.md) and [native acceptance](../evaluations/p1-completion.md): shared SQLite/files constraints, combined ledger transaction crashes, both conversion directions, atomic activation and retained reopen; [JSON preservation follow-up](../evaluations/p1-persisted-json.md) covers literal object keys, unchanged receipts and history conversions | complete |
| P1-05 Budget ledger | P1-04 | [Implementation and tests](03-storage-and-budget.md#p1-05--ledger-increments) | [Accounting/history](../development/p1-accounting-history.md) and [native acceptance](../evaluations/p1-completion.md): root/child/daily limits, immutable adjustments, unresolved liabilities and actual root/helper/compaction/child HTTP admission races | complete |
| P1-06 Projections and history | P1-02/04 | [Implementation and tests](02-engine-state-and-capture.md#p1-06--projections-and-history) | [Accounting/history](../development/p1-accounting-history.md) and [native acceptance](../evaluations/p1-completion.md): atomic folds/activation, authorized snapshot filters/gaps and fresh-process rebuild preserving an actual unknown process, paused child and late charge | complete |
| P2-01 Repository/context | P1-03/04 | [Implementation and tests](04-context-and-instructions.md#p2-01--discovery-and-initial-assembly) | [P2 acceptance](../evaluations/p2-completion.md): bounded Git/worktree observation, scoped parent/nested instructions, captured selection, envelope sealing and stale dependency/send fences | complete |
| P2-02 OpenRouter gateway | P1-05, P0-09 | [Implementation and tests](05-openrouter-and-session-loop.md#p2-02--provider-path) | [P2 acceptance](../evaluations/p2-completion.md): bounded codec/streaming, controller-owned retries with fresh reservations/predecessors and original deadlines, raw-only HTTP errors, retained liability and capped two-model live compatibility | complete |
| P2-03 Autonomy and grants | P1-01/04, P0-05/06 | [Implementation and tests](06-windows-tools-and-recovery.md#p2-03--effective-autonomy-and-grants) | [Authority acceptance](../evaluations/p2-policy-completion.md): presets, durable questions, exact grants and current native/retained broker enforcement; general OS isolation and installed CLI retain their owning gates | complete |
| P2-04 Execution/tools | P2-03, P1-03/04 | [Implementation and tests](06-windows-tools-and-recovery.md#p2-04--tools-and-windows-worker) | [P2 acceptance](../evaluations/p2-completion.md): prepared/versioned native files, explicit process profiles, owned PTY/process trees, bounded output/deadlines/counts and per-effect receipts | complete |
| P2-05 Session loop | P2-01…04, P0-08/09 | [Implementation and tests](05-openrouter-and-session-loop.md#p2-05--retained-codex-loop-with-vcp-boundaries) | [P2 acceptance](../evaluations/p2-completion.md): retained admitted loop, complete-response tools, bounded resource-aware scheduling, scoped-generation fences, empty-response failure, explicit capability states and durable results | complete |
| P2-06 Verification/completion | P2-05 | [Implementation and tests](05-openrouter-and-session-loop.md#p2-06--verification-and-honest-completion) | [P2 acceptance](../evaluations/p2-completion.md): actual project/Cargo/documentation checks, stale and failed evidence, source applicability, unresolved effects and current-cost completion | complete |
| P2-07 Recovery and close handling | P2-04/05, P1-06 | [Implementation and tests](06-windows-tools-and-recovery.md#p2-07--pause-and-unknown-effect-reconciliation) | [P2 acceptance](../evaluations/p2-completion.md): actual console close/forced-kill distinction, bounded cancellation, quiescent observation-only file/process reconciliation, PID safety, unknown charges, fresh resume validation and no replay | complete |
| P2-08 Context continuity | P2-01/05, P1-05/06 | [Implementation and tests](04-context-and-instructions.md#p2-08--refresh-and-compaction) | [P2 acceptance](../evaluations/p2-completion.md): complete-pair compaction, preserved corrections/effects/costs, explicit handoff packets with current base/diff/budget and opaque-field omissions, validated reassembly and fresh-process continuity | complete |
| P3-01 Structured CLI | P2-06/07/08 | [Implementation and tests](07-cli-and-inspection.md#p3-01--structured-command-surface) | [Native CLI and owner-control evidence](../development/p3-cli.md#executable-completion--2026-09-19); E01 structured mode, required input, broken pipe, cancellation and ambiguous outcomes | Complete |
| P3-02 Interactive terminal | P3-01 | [Implementation and tests](07-cli-and-inspection.md#p3-02--terminal-workflow) | [Native console and ConPTY qualification](../development/p3-terminal.md): Unicode/resize/close, bounded output, scoped answers, pause/steer/inspect/resume/cancel; P2-07 cancelled-producer receipt correction preserves uncertain money | complete |
| P3-03 Inspectors | P1-06, P2-06, P3-01 | [Implementation and tests](07-cli-and-inspection.md#p3-03--evidence-inspectors) | [Inspector evidence](../development/p3-inspection.md): paged canonical relationships, exact captured bytes, current access/retention and rebuild parity; memory/retention UI remains later work | Complete |
| P3-04 Workspace continuation | P3-01, P2-07 | [Implementation and tests](07-cli-and-inspection.md#p3-04--workspace-continuation) | [Native continuation evidence](../development/p3-continuation.md): revision-bound chooser, explicit resume, current instructions, partial children/effects/accounting and trust-revoking local rebind; U06 | Complete |
| P3-05 History and pruning CLI | P3-03, P5-07 | [Implementation and tests](10-history-and-pruning.md#p3-05--browsing-and-cli-controls) | U05; [native history/prune controls and stable authorized paging](../development/p3-history.md#qualification) | Complete |
| P3-06 Portable environment CLI | P3-04, P5-09/10 | [Implementation and tests](11-encrypted-portability.md#p3-06--user-commands-and-diagnostics) | U04; no secret in model prompts/logs/argv, missing-key/setup failures actionable, published ciphertext distinguished from verified restore; [actual two-Windows U04 passed](../development/p3-onedrive-qualification.md) | Complete |
| P4-01 Engine connection | P9-02/03 | [Implementation and tests](18-deferred-vscode.md#p4-01--connection-and-workspace-mapping) | [Connection, trust, moved roots and observer reload acceptance](../development/editor-connection.md#p4-01-acceptance) | Complete |
| P4-02 Session/child views | P4-01 | [Implementation and tests](18-deferred-vscode.md#p4-02--task-and-child-views) | [Acceptance evidence and limits](../development/editor-tasks.md#qualification): bounded canonical views, access-checked presentation, CLI parity, dropped subscriptions, guarded durable actions and reload | Complete |
| P4-03 Versioned edits | P4-01, P2-04 | [Implementation and tests](18-deferred-vscode.md#p4-03--versioned-document-edits) | [E05/R02 native editor evidence](../development/editor-edits.md#qualification): both stores, typing conflicts, partial receipts, save/undo/reload, no replay and persisted buffer fence | Complete |
| P4-04 Inspectors | P4-02, P3-03/05 | [Implementation and tests](18-deferred-vscode.md#p4-04--inspectors) | [Native inspector acceptance](../development/editor-inspectors.md#p4-04-acceptance): governed paging, purge/trust invalidation, observer reload, exact optimizer apply/rollback and encrypted publication on both stores | complete |
| P4-05 Packaging | P4-01…04 | [Implementation and tests](18-deferred-vscode.md#p4-05--packaging-and-compatibility) | [VSIX/native package and startup qualification](../development/editor-packaging.md): actual installation, failed-update recovery, uninstall and buffer races on both stores | complete |
| P5-01 Munarium governance | P1-04, P0-02/07 | [Implementation and tests](08-memory-and-ingestion.md#p5-01--governed-writes) | [Native governed-memory evidence](../development/p5-governed-memory.md): Files/SQLite retries, immutable versions, scoped retained evidence, all six classes, contradictions/corrections, historical access and projection repair; E13/M01 | Complete |
| P5-02 Activity/evidence ingestion | P5-01, P1-03 | [Implementation and tests](08-memory-and-ingestion.md#p5-02--activity-and-evidence-ingestion) | External actor unknown where unobserved; no cross-workspace leakage; [native qualification](../development/p5-ingestion.md) | Complete |
| P5-03 Tantivy adapter | P0-02, P5-02 | [Implementation and tests](09-local-search-and-generations.md#p5-03--lexical-retrieval) | Lexical recall, delete/merge, version and watermark cases; [native qualification](../development/p5-lexical.md) | Complete |
| P5-04 Local embeddings/DiskANN | P0-02, P5-02, P1-05 | [Implementation and tests](09-local-search-and-generations.md#p5-04--local-vectors-and-diskann) | U09/M03/M04; [native network boundary, exact-search oracle, reopen/filter and resource qualification](../development/p5-vectors.md) | Complete |
| P5-05 Publication/recovery | P5-03/04, P1-04 | [Implementation and tests](09-local-search-and-generations.md#p5-05--coherent-publication-and-recovery) | M02/M05/M06; [atomic activation, process-kill recovery and cleanup qualification](../development/p5-publication.md) | Complete |
| P5-06 Retrieval/inspection | P5-05, P3-03 | [Implementation and tests](09-local-search-and-generations.md#p5-06--hybrid-query-and-inspection) | E20/M04; [canonical hybrid retrieval, dispatch fences and native quality qualification](../development/p5-retrieval.md) | Complete |
| P5-07 Pruning/retention | P5-05/06, P1-03/04 | [Implementation and tests](10-history-and-pruning.md#p5-07--retention-engine) | U05/M05/M07; [typed selection, replay-base cleanup, accounting protection and native crash qualification](../development/p5-retention.md#qualification) | Complete |
| P5-09 Portable snapshots | P5-05/07, P1-04 | [Implementation and tests](11-encrypted-portability.md#p5-09--key-lifecycle-and-publication) | U04/M08/I-19; interrupted/error paths never expose plaintext or keys, no recipient substitution, encryption finalized before publication; [actual two-Windows U04 passed](../development/p3-onedrive-qualification.md) | Complete |
| P5-10 Restore and handoff | P5-09, P3-04 | [Implementation and tests](11-encrypted-portability.md#p5-10--restore-and-sequential-handoff) | U04 on two Windows environments; wrong/missing key and tampering rejected, recovery/rotation verified, no lost records or overwritten divergent work; [actual two-Windows U04 passed](../development/p3-onedrive-qualification.md) | Complete |
| P5-08 Integrated memory acceptance | P5-01…07, P5-09/10 | [Implementation and tests](15-integration-and-release.md#p5-08--integrated-memory-acceptance) | E13/E14/E20/M01–M08/U04/U05/U09; [frozen three-strategy comparison, bounded recent recall, native hybrid and encrypted restore/retention qualification](../evaluations/p5-08-integrated-memory.md) | Complete |
| P6-01 Group registry | P2-02, P5-06 | [Implementation and tests](12-routing-and-optimization.md#p6-01--versioned-registry) | Group/profile distinction, unknown capability/price and stale research handled explicitly; [construction and qualification disposition](../evaluations/p6-completion.md) | Complete |
| P6-02 Routing/profiles | P6-01, P1-05 | [Implementation and tests](12-routing-and-optimization.md#p6-02--deterministic-profile-policy) | Deterministic baseline plus bounded contract/thin Rust adapter for actual Jev through OpenRouter and qualified permitted conventional-LLM fallback; schema/probability semantics, scope, pause, root cost and quality floor enforced; [construction and qualification disposition](../evaluations/p6-completion.md) | Complete |
| P6-03 Escalation/handoff | P6-02, P2-05/08 | [Implementation and tests](12-routing-and-optimization.md#p6-03--escalation-and-model-handoff) | E04/E11/E12/U07; bounded escalation/review advice cannot suppress required checks, lose constraints or orphan tool results; [construction and qualification disposition](../evaluations/p6-completion.md) | Complete |
| P6-05 Project optimizer | P6-03, P5-06, P3-02/03 | [Implementation and tests](12-routing-and-optimization.md#p6-05--optimize-workflow) | U07; local workflow survives disabled advice; all evaluator overhead and sparse/biased/pruned history labelled; no silent spend, authority or deletion changes; [construction and qualification disposition](../evaluations/p6-completion.md) | Complete |
| P6-04 Profile qualification | P6-03/05, P5-08 | [Implementation and tests](12-routing-and-optimization.md#p6-04--profile-qualification) | E19/U07 with held-out deterministic/actual-Jev/conventional-LLM comparisons, false negatives, calibration limits and all failed-attempt/child/evaluator costs; unqualified purposes stay disabled; [construction and qualification disposition](../evaluations/p6-completion.md) | Complete |
| P7-01 Skill discovery | P2-01/03 | [Implementation and tests](13-skills-and-mcp.md#p7-01--discovery-and-activation) | E02/E03/U08; [bounded discovery, canonical activation, authority/reopen fences and native CLI qualification](../evaluations/p7-01-skills.md); no executable config-import dependency | Complete |
| P7-02 Bundled development skills | P7-01, P2-06 | [Implementation and tests](13-skills-and-mcp.md#p7-02--built-in-skill-catalog) | U01–U03/U08; [21-family packaged catalog](../development/p7-builtin-skills.md), [workflow refinements](../evaluations/p7-02-workflow.md), [partial native toolchain evidence](../evaluations/p7-02-native-toolchains.md); [selected live completion and retained failures](../evaluations/p7-02-completion.md); unavailable toolchains remain unvalidated | Complete |
| P7-03 MCP | P7-01, P2-03/04 | [Implementation and tests](13-skills-and-mcp.md#p7-03--mcp-lifecycle-and-tool-calls) | [Governed stdio qualification](../evaluations/p7-03-stdio.md) covers local protocol, identity, source fences and unknown outcomes; [canonical HTTP qualification](../evaluations/p7-03-http.md) covers remote tools, scoped credentials, native trust and both-store fault recovery; [content qualification](../evaluations/p7-03-content.md) covers resources, prompts and scoped cache; [exact numeric/schema qualification](../evaluations/p7-03-numeric.md) covers ADR-028; [final fault qualification](../evaluations/p7-03-final-faults.md) closes reply-before-receipt interruption and native HTTP 401 proofs | Complete |
| P7-04 Task graph/worktrees | P6-03, P2-07 | [Implementation and tests](14-visible-delegation.md#p7-04--graph-and-workspace-ownership) | E12/E15/U06; [graph/workspace implementation](../development/p7-child-workspaces.md), isolated sibling writes, dependencies, shared-budget race and [qualified exploration comparison](../evaluations/p7-interruption-exploration-2026-09-22.md); baseline preferred without useful helper gain; no silent shared writes or orphan charges | Complete |
| P7-05 Integration/review | P7-04, P2-06 | [Implementation and tests](14-visible-delegation.md#p7-05--integration-and-review) | U02/U03/E15; [paired review/generation quality](../evaluations/p7-qwen38-reasoning-budget-2026-09-22.md), current-parent checks and [write/receipt and verification-publication interruption](../evaluations/p7-interruption-exploration-2026-09-22.md); failed full run and passing focused repairs retained separately | Complete |
| P7-06 Visible progress and recovery | P7-04/05, P3-02/04 | [Implementation and tests](14-visible-delegation.md#p7-06--commentary-controls-and-recovery) | U06; [live terminal controls](../evaluations/p7-live-qualification-2026-09-22.md), [independent sibling controls, child-write crash, noisy/quiet consumer loss and cursor recovery](../evaluations/p7-interruption-exploration-2026-09-22.md); canonical history and known/unknown charges retained | Complete |
| P8-01 Native Windows matrix | P3-06, P6-04, P7-02/03/06 | [Implementation and tests](15-integration-and-release.md#p8-01--native-windows-support-matrix) | E07/E08/E18/U09; [native matrix and CPU measurements](../evaluations/p8-native-qualification-2026-09-22.md) recorded; clean OS and full packaged coverage open; [owner-directed closure](../adr/042-owner-directed-p8-closure.md) accepts the recorded gaps for development progression | Complete |
| P8-02 Recovery/portability campaign | P7-06, P5-07/10, P3-05/06 | [Implementation and tests](15-integration-and-release.md#p8-02--recovery-and-portability-campaign) | E10/E14/M02/M07/M08/U04–U06; [local follow-up](../evaluations/p8-local-recovery-followup-2026-09-22.md) maps prior activation/corruption receipts and adds forty process kills, capacity faults, divergence, content authority and actual cloud-interruption recovery followed by exact-package restore; physical-volume and production scope remains open; [owner-directed closure](../adr/042-owner-directed-p8-closure.md) accepts the recorded gaps for development progression | Complete |
| P8-03 Full-history/export review | P3-05/06, P5-07/10 | [Implementation and tests](15-integration-and-release.md#p8-03--full-history-and-encryption-review) | U04/U05/I-19; [local packaged follow-up](../evaluations/p8-history-security-followup-2026-09-22.md) passes typed-secret exclusion, independent crypto and MCP compaction/reopen/revocation/purge on both stores; production scope remains open and independent-machine recovery stays skipped; [owner-directed closure](../adr/042-owner-directed-p8-closure.md) accepts the recorded gaps for development progression | Complete |
| P8-04 Windows distribution | P8-01, P5-10 | [Implementation and tests](15-integration-and-release.md#p8-04--windows-distribution) | Unsigned candidate, standalone installer and model provisioner implemented; [allowance candidate](../evaluations/p8-allowance-package-2026-09-23.md) exact-artifact same-host smoke passes; fresh-machine transfer remains open; [owner-directed closure](../adr/042-owner-directed-p8-closure.md) accepts the recorded gaps for development progression | Complete |
| P8-06 Upstream maintenance | P7-06, P0-07, P2-05 | [Implementation and tests](15-integration-and-release.md#p8-06--upstream-maintenance-rehearsal) | R08; [adjacent Codex rehearsal](../evaluations/p8-upstream-maintenance-2026-09-22.md), unchanged 36-patch replay, retained/VCP suites and measured effort; initial dialog interventions retained, launch fix qualified unattended | Complete |
| P8-05 Owner sign-off and release evaluation | P6-04, P8-01…04, P8-06 | [Implementation and tests](15-integration-and-release.md#p8-05--owner-acceptance-and-release-evaluation) | [Scorecard](../evaluations/p8-release-scorecard-2026-09-22.md), [P8 gaps](../evaluations/p8-acceptance-gaps-2026-09-22.md), [owner packet](../evaluations/p8-owner-acceptance-package-2026-09-22.md) and [approved campaign](../evaluations/p8-approved-campaign-2026-09-23.md); FR/I/U01–U09 final qualification and quality scoring remain incomplete; [owner-directed closure](../adr/042-owner-directed-p8-closure.md) accepts the recorded gaps for development progression | Complete |
| P9-01 Public protocol | P8-05, P1-02 | [Implementation and tests](17-deferred-api-and-sdk.md#p9-01--public-protocol) | [Public wire and initial adapter contract](../development/public-protocol.md), [schema/identity decision](../adr/043-public-protocol-schema-and-identity.md), [PR #134](https://github.com/iokaio/vcp/pull/134); 58 native tests, eight generator contracts, TypeScript and 13 independent schema fixtures; remaining live methods belong to P9-02 | Complete |
| P9-02 Server/attach | P9-01 | [Implementation and tests](17-deferred-api-and-sdk.md#p9-02--local-server-and-attachment) | [Native acceptance record](../development/local-attachment.md#p9-02-acceptance-and-next-dependency): authenticated attachment, leases, owned execution and pause/loss, pending input, durable retry, snapshot/event recovery, bounded readers, CLI/API parity, scoped inspection/governance/export/physical forgetting; [ADR-054](../adr/054-scoped-public-retention.md) records final both-store bridge and real MCP qualification. Editor adapters belong to P4 and SDK consumer qualification to P9-03. | Complete |
| P9-03 TypeScript SDK | P9-02 | [Implementation and tests](17-deferred-api-and-sdk.md#p9-03--sdk) | [SDK qualification](../development/typescript-sdk.md), [ADR-055](../adr/055-typescript-local-sdk.md): 29 package/runtime tests, clean offline build/consumer, four compiled tests on both stores, seven runnable examples, original-command/effect evidence and explicit cancellation/disposal boundaries | Complete |
| P10-01 Hooks | P8-05, P7-03 | [Implementation and tests](19-deferred-extensions-and-platforms.md#p10-01--hooks) | E16/R06 timeout, recursion, rewrite and recovery cases | Planned |
| P10-02 Configuration imports | P8-05, P7-01/03 | [Implementation and tests](19-deferred-extensions-and-platforms.md#p10-02--configuration-import) | Unsupported fields reported; imported data grants no authority | Planned |
| P10-03 Optional observers | P8-05, P7-06 | [Implementation and tests](19-deferred-extensions-and-platforms.md#p10-03--optional-observers) | Measured benefit, root cost and intervention-rate accounting | Planned |
| P10-04 Other environments | P8-05 | [Implementation and tests](19-deferred-extensions-and-platforms.md#p10-04--other-execution-environments) | Separate per-host path/process/credential and installation matrices | Planned |

## First-release dependency closure

The [Markov](#markov-follow-up-readiness) and
[developer-workflow](#developer-workflow-follow-up-readiness) ledgers track
pending increments inside existing owners. The architecture dependency graph and
historical completion evidence remain unchanged; new consumer behavior needs its
own implementation and qualification evidence.

The transitive dependency closure of [P8-05](15-integration-and-release.md#p8-05--owner-acceptance-and-release-evaluation) includes all 56 first-release tasks and none of the deferred 12. Maintain this property when changing dependencies.

Special staging relationships:

- [P1-06](02-engine-state-and-capture.md#p1-06--projections-and-history) follows [P1-04](03-storage-and-budget.md#p1-04--storage-increments) even though its file appears earlier in the reading order.
- [P2-08](04-context-and-instructions.md#p2-08--refresh-and-compaction) follows [P2-05](05-openrouter-and-session-loop.md#p2-05--retained-codex-loop-with-vcp-boundaries); the context file is not a whole-file prerequisite of the loop.
- [P5-06](09-local-search-and-generations.md#p5-06--hybrid-query-and-inspection) requires the common inspector interface [P3-03](07-cli-and-inspection.md#p3-03--evidence-inspectors); memory-specific inspection completes with retrieval.
- [P3-05](10-history-and-pruning.md#p3-05--browsing-and-cli-controls) and [P3-06](11-encrypted-portability.md#p3-06--user-commands-and-diagnostics) follow the relevant P5 implementations, while basic CLI work starts earlier.
- [P5-08](15-integration-and-release.md#p5-08--integrated-memory-acceptance) is the early stage of segment 15 and precedes [P6-04](12-routing-and-optimization.md#p6-04--profile-qualification); it does not require final P8 completion.
- [P7-04](14-visible-delegation.md#p7-04--graph-and-workspace-ownership) can begin after [P6-03](12-routing-and-optimization.md#p6-03--escalation-and-model-handoff) and [P2-07](06-windows-tools-and-recovery.md#p2-07--pause-and-unknown-effect-reconciliation); final routing qualification and delegation integration are jointly rechecked for release.

## Markov follow-up readiness

Plan revision 13 adds the [Markov execution supplement](21-markov-integration.md).
This separate ledger prevents earlier completed P2/P5 rows from hiding unfinished
refactoring. M1's task-transition, numerical, action/effect-observation, exact
charge-reward, fit-provenance, held-out order-comparison, exact reward-mapping and consumed-value replay increments are implemented. M2 shadow construction and
M3 read-only reporting are implemented; M4 exited with recorded rejection and
disabled candidates. No statistical default is qualified. Labels M1–M10 are supplement
increments, not the architecture's M01–M08 memory tests or new task IDs. When
implementation lands, update these rows with evidence and refresh affected task
acceptance. Optional candidates may exit as explicitly rejected/deferred after
evaluation; they must not silently disappear from release review.

| Increment | Existing owners / prerequisites | Remaining acceptance | State |
|---|---|---|---|
| M1 evidence/analysis foundation | P6-02; supporting P1-06/P5-07/P5-10 refactors | [Task transitions](../evaluations/p6-transition-evidence.md), [numerical kernels](../evaluations/p6-markov-kernels.md), [causal action observations](../evaluations/p6-action-evidence.md), [exact charges/rewards](../evaluations/p6-charge-rewards.md), [fit provenance](../evaluations/p6-fit-provenance.md), [held-out order comparison](../evaluations/p6-heldout-order-comparison.md), [reward mapping](../evaluations/p6-reward-mapping.md), [consumed-value replay](../evaluations/p6-consumed-value-replay.md) | implemented |
| M2 escalation integration | P6-03 after M1 | [Canonical local/remote shadow runtime](../evaluations/p6-local-shadow-runtime.md), exact-cycle rules and separate provenance; serving influence remains unqualified | implemented shadow construction |
| M3 optimizer forecasts | P6-05 after P6-03 implementation and M1 | [Saved read-only forecasts and drift/compaction diagnostics](../evaluations/p6-saved-forecast-diagnostics.md); no policy mutation or predictive qualification | implemented construction |
| M4 qualification/consumption | P6-04 and P6-02/03; M2/M3 and P5-08 | [Original offline rejection](../evaluations/p6-markov-qualification.md) retains historical remote not-run rows; [subsequent actual Jev/conventional comparison](../evaluations/p6-decision-comparison.md) records measured rejection, no qualified estimate or behavioral enablement | complete through disabled-candidate outcome |
| M5 context/retrieval refactors | P2-01/08, P5-06/08; current baseline, M1 for compaction diagnostics | Baseline retained; graph/co-change/fusion candidates have no qualified comparison | explicitly deferred for first release; [disposition](../evaluations/p8-continuation-2026-09-23.md#supplement-dispositions) |
| M6 verification refactor | P2-06; current baseline and M1 observations | Baseline required checks and current-revision completion retained; statistical ordering lacks matched evidence | explicitly deferred for first release; [disposition](../evaluations/p8-continuation-2026-09-23.md#supplement-dispositions) |
| M7 proposals/bursts | P6-05/03/04 after M3/M4 | Offline supported-action policy tables, selected diffs/rollback and authorized trials; endpoint burst evidence or recorded abstention | explicitly deferred: no qualified downstream action/outcome model or adequate endpoint evidence |
| M8 child reuse | P7-04/05/06 under existing dependencies | M4 supplies no qualified forecast; existing atomic root limits and integrated-parent checks remain required | explicitly deferred; [disposition](../evaluations/p8-continuation-2026-09-23.md#supplement-dispositions) |
| M9 synthetic/release campaign | Segment 16 from M1; final P8-01/02/05 after enabled consumers/P7 | Numerical fixtures, [seeded boundary campaign](../evaluations/p8-seeded-boundaries-2026-09-23.md) and [provider/child scenarios](../evaluations/p8-seeded-provider-children-2026-09-23.md); packaged delegation and remaining release evidence stay open | partial; [disposition](../evaluations/p8-continuation-2026-09-23.md#supplement-dispositions) |
| M10 regime observer | P10-03 after P8-05/P7-06 | Qualified local producer reused with bounded cursors, pause/recovery and matched usefulness evidence | planned |

## Developer workflow follow-up readiness

Revision 14 adopts the selected scope in [plan 22](22-cursor-improvements.md) and
[plan 23](23-claudecode-improvements.md), informed by the preserved
[Cursor](../research/cursor.md) and [Claude Code](../research/claudecode.md) research.
The research labels are supplement references, not new architecture tasks. Each
row below belongs to its P7 refinement or P8 qualification owner. Completed
P2/P3/P5/P6/P7 code is a supporting boundary, not a newly reopened owner.
The P7-02 source navigation, debug guidance, execution ceilings and output
refinements have selected workflow qualification; see the
[workflow evidence](../evaluations/p7-02-workflow.md) and [live completion](../evaluations/p7-02-completion.md).
The [delegation follow-up](../evaluations/p7-interruption-exploration-2026-09-22.md)
records P7 helper, cleanup and interruption acceptance and its passing final
native gate. P8 additions below retain their separate open gates.

| Increment | Remaining owner and prerequisite | Required acceptance | State |
|---|---|---|---|
| [CR-02a / CC-04 bounded search and reads](22-cursor-improvements.md#cr-02a--bounded-search-and-ranged-reads) | P7-02; existing P2 tools, then P8-01 | Literal-default compatibility, explicit bounded regex and paging, deterministic authorized discovery, source/version evidence, U01/U02 usefulness | complete |
| [CR-06 debug guidance](22-cursor-improvements.md#cr-06--evidence-first-debug-guidance) | P7-02; existing review-debug/testing skills | Reproduce, fix, verify current result and remove only owned instrumentation; missing checks remain not-run | complete |
| [CR-03 helper defaults](22-cursor-improvements.md#cr-03--small-helper-role-defaults) | P7-04/06; existing graph and routing | Bounded useful read-only helpers, inherited authority/root budget, concise attributed progress and single-agent path | qualified; [broad exploration prefers baseline](../evaluations/p7-interruption-exploration-2026-09-22.md), paired review retained; final P7 native gate passed |
| [CR-08 / CC-15 worktree readiness and cleanup](22-cursor-improvements.md#cr-08--worktree-readiness-and-safe-cleanup) | P7-04/05/06; existing snapshots/markers, then P8-02 | Actionable setup state, exact registered-root cleanup, preserved live references/history and interrupted/locked-root recovery | P7 scope qualified; final P7 native gate passed; broader packaged P8 fault schedule remains open |
| [CR-12a / CC-14a review findings](22-cursor-improvements.md#cr-12a--useful-local-review-findings) | P7-05; P7-02 shared review guidance, then P8-05 | Evidence-backed current-revision findings, honest causality/verification, U02 quality and integrated-parent checks | implemented and qualified; [Luna review passes](../evaluations/p7-review-generation-quality-2026-09-22.md); [Qwen 3.8 review pair and generation pass with recorded caveats](../evaluations/p7-qwen38-reasoning-budget-2026-09-22.md) |
| [CC-02a execution ceilings](23-claudecode-improvements.md#cc-02a--declared-foreground-execution-ceilings) | P7-02; existing process/verification broker, then P8-01/02 | Trusted bounded long foreground checks; default compatibility, responsive pause/cancel, current-result and output-cap outcomes | complete |
| [CC-03 Windows process output](23-claudecode-improvements.md#cc-03--windows-process-output-and-outcomes) | P7-02; existing raw outcome/artifacts, then P8-01/03 | Declared decoding, explicit decoding loss and accurate raw outcomes without rewriting exit status or verification pass rules | complete |
| [CR-10a / CC-08 boundary qualification](22-cursor-improvements.md#cr-10a--qualify-existing-trust-boundaries) | P8-02 with P8-01; existing policy/native/MCP/child paths | Independent hostile-path/cleanup cases and normal granted developer edits; fix demonstrated gaps without broad new prompts | [native and MCP content checks pass](../evaluations/p8-local-recovery-followup-2026-09-22.md); prior child/junction/cleanup receipts mapped; production-artifact scope remains open |
| [CR-13 retained context and MCP evidence](22-cursor-improvements.md) | P8-03; existing artifacts, history, compaction and MCP | Access/retention enforced after compaction and reopen, truthful MCP identity/state, no new recall subsystem | [Both-store packaged local qualification passes](../evaluations/p8-history-security-followup-2026-09-22.md); production-artifact and latency acceptance remain open |
| Packaged workflow acceptance | P8-01/02/03/05 under their exact task dependencies | Current artifact, both stores where state changes, live U01–U03/U06/U08 and all existing release gates; concise usable developer workflow | planned |

Implement the P7 refinements with their owning milestones, then qualify the
combined package under P8. Follow explicit increment prerequisites inside the
supplements; file order introduces no dependency. New records or tool/catalog
revisions require affected reopen, retention, authority and compatibility cases.
Use the existing M5/M6 comparisons when changed context or verification behavior
affects them; no selected refinement depends on an unqualified statistical fit.

The deferred/non-selected tables in plans 22/23 are the disposition record for
larger candidates. They add no release gate, command, dependency or automatic
implementation commitment. Reconsider one only through separately scoped work
with a demonstrated developer benefit. At P8-05, record selected implementation
and evidence plus the deferred disposition; do not mark a selected gap complete
merely because the research was reviewed.

## Owner-answer coverage

All owner answers remain binding within their stated release scope. Initial
case IDs are planned product observations, not passing harness results.

| Answer | Owning plans | Initial evidence |
|---|---|---|
| A01 License and distribution | [15](15-integration-and-release.md) | R08, U01–U09 and package/notice inventory |
| A02 Complete coding, routing and memory | [05](05-openrouter-and-session-loop.md), [08](08-memory-and-ingestion.md), [12](12-routing-and-optimization.md) | U01–U09 |
| A03 CLI first, public clients later | [07](07-cli-and-inspection.md), later [17](17-deferred-api-and-sdk.md)/[18](18-deferred-vscode.md) | E09, U01; public variants deferred |
| A04 Native Windows | [01](01-upstream-feasibility.md), [06](06-windows-tools-and-recovery.md), [15](15-integration-and-release.md) | E07, E08, R04 |
| A05 Codex reuse and local Munarium | [01](01-upstream-feasibility.md), [08](08-memory-and-ingestion.md), [09](09-local-search-and-generations.md) | R02–R05, R08, U09 |
| A06 Portable state and backend preference | [03](03-storage-and-budget.md), [11](11-encrypted-portability.md) | M01, M08, U04 |
| A07 Scoped history and pruning | [07](07-cli-and-inspection.md), [10](10-history-and-pruning.md) | M05, U05 |
| A08 Automatic governed memory | [08](08-memory-and-ingestion.md), [10](10-history-and-pruning.md) | M01, M05, U05 |
| A09 Local memory, OpenRouter coding | [05](05-openrouter-and-session-loop.md), [09](09-local-search-and-generations.md) | E11, M03, U09 |
| A10 Profiles and interactive optimization | [12](12-routing-and-optimization.md) | E12, E19, U07 |
| A11 Autonomy controls | [06](06-windows-tools-and-recovery.md) | E06, R03 |
| A12 Full history and age notification | [02](02-engine-state-and-capture.md), [10](10-history-and-pruning.md) | E17, U05 |
| A13 Pause and workspace resume | [06](06-windows-tools-and-recovery.md), [07](07-cli-and-inspection.md), [14](14-visible-delegation.md) | E08, E10, U06, including pause without closing |
| A14 Instructions, skills, MCP | [04](04-context-and-instructions.md), [13](13-skills-and-mcp.md) | E02, E03, E16, U08 |
| A15 Visible delegation | [14](14-visible-delegation.md) | E15, U02, U03, U06 |
| A16 Analysis, review and generation | [01](01-upstream-feasibility.md), [15](15-integration-and-release.md), [16](16-test-fixtures-and-acceptance.md) | U01, U02, U03; [initial fixtures](../development/experiment-fixtures.md) |
| A17 Plaintext active state, encrypted vault | [01](01-upstream-feasibility.md), [11](11-encrypted-portability.md), [15](15-integration-and-release.md) | M08, U04, I-19 |

## Functional requirement coverage

| Requirement | Implementation segments | Primary evidence |
|---|---|---|
| FR-01 Shared engine and CLI first | [02](02-engine-state-and-capture.md), [07](07-cli-and-inspection.md); later [17](17-deferred-api-and-sdk.md)/[18](18-deferred-vscode.md) | E01/E09, internal R01; later public-client parity |
| FR-02 OpenRouter | [05](05-openrouter-and-session-loop.md), [12](12-routing-and-optimization.md) | E11/R05/U07 |
| FR-03 Cost profiles | [03](03-storage-and-budget.md), [12](12-routing-and-optimization.md) | E12/E19/U07 |
| FR-04 Governed memory | [08](08-memory-and-ingestion.md), [10](10-history-and-pruning.md) | E13/M01/M05/U05 |
| FR-05 Tantivy/DiskANN | [09](09-local-search-and-generations.md) | M02/M03/M04/M05/M06/E20/U09 |
| FR-06 Transparency | [02](02-engine-state-and-capture.md), [07](07-cli-and-inspection.md), [14](14-visible-delegation.md) | E17/U05/U06 |
| FR-07 Reliable edits | [06](06-windows-tools-and-recovery.md), [14](14-visible-delegation.md); later [18](18-deferred-vscode.md) | E05/E07/E15/U03 |
| FR-08 Pause/resume | [06](06-windows-tools-and-recovery.md), [07](07-cli-and-inspection.md), [14](14-visible-delegation.md) | E04/E08/E10/U06 |
| FR-09 Bounded spend | [03](03-storage-and-budget.md), [05](05-openrouter-and-session-loop.md), [12](12-routing-and-optimization.md), [14](14-visible-delegation.md) | E12/U07; actual request/effect and ledger observations |
| FR-10 Recovery | [03](03-storage-and-budget.md), [06](06-windows-tools-and-recovery.md), [11](11-encrypted-portability.md) | E10/E14/M02/M07/M08/U04/U06 |
| FR-11 AGENTS.md/skills/MCP | [04](04-context-and-instructions.md), [13](13-skills-and-mcp.md) | E02/E03/E16/U08 |
| FR-12 Visible delegation | [14](14-visible-delegation.md) | E12/E15/U02/U03/U06 |
| FR-13 Maintainable reuse | [01](01-upstream-feasibility.md), [15](15-integration-and-release.md) | Applicable R01–R08 and source/notice/update evidence |
| FR-14 Portable encrypted environment | [03](03-storage-and-budget.md), [11](11-encrypted-portability.md), [15](15-integration-and-release.md) | M01/M08/U04/I-19; developer recovery and vault observation |
| FR-15 Full history/pruning | [02](02-engine-state-and-capture.md), [10](10-history-and-pruning.md) | U05/E17/M05 |
| FR-16 Local memory compute | [01](01-upstream-feasibility.md), [09](09-local-search-and-generations.md), [15](15-integration-and-release.md) | U09/M03/M04 and actual network/resource observations |
| FR-17 Project optimization | [12](12-routing-and-optimization.md) | E19/U07 and effective policy before/after/rollback |

## Invariant verification ownership

| Invariant | Owner segments | Independent assertion |
|---|---|---|
| I-01 No model/content-granted authority | 04, 06, 13 | Hostile data cannot produce a permitted dispatch |
| I-02 Validated authorized durable dispatch | 05, 06 | Broker effects correlate to immutable authorized intent |
| I-03 Idempotency identity | 02, 03 | Repeated command returns original result; changed payload rejected |
| I-04 Reservation before billable request | 03, 05 | Transport request count has committed reservation for every attempt |
| I-05 Root attribution | 03, 12, 14 | Supporting and child calls included once; unknown charges retained |
| I-06 Preserve user changes | 04, 06, 14; later 18 | Independent initial/final staged/unstaged/untracked comparison |
| I-07 Durable memory acceptance | 03, 08, 09 | Accepted canonical record survives restart; lag reported |
| I-08 Scoped sourced retrieval | 08, 09, 10 | Every passage resolves to authorized current source/version |
| I-09 Compaction preserves history | 02, 04 | Original full artifacts unchanged after projection compaction |
| I-10 Stop scheduling on cancellation | 05, 06, 14 | No new dispatch after stop boundary; residual effects visible |
| I-11 No blind uncertain-effect replay | 06, 11, 13, 14 | Independent non-idempotent marker is not duplicated after reopen |
| I-12 Current completion evidence | 02, 05, 14 | Later edits invalidate checks; merged state actually verified |
| I-13 Backend contract parity | 03, 08, 11 | Shared fixtures produce equivalent canonical outcomes |
| I-14 Explicit unknown states | 02, 05, 07, 09, 12 | Missing capability/usage/content/index state not represented as success |
| I-15 Complete restore and divergence | 11 | Corrupt/incomplete/stale/competing snapshots never silently overwrite |
| I-16 Local embeddings/indexing | 01, 09, 15 | Real CPU embedding/index tests with embedding-network access disabled |
| I-17 Protected pruning dependencies | 03, 10, 11 | Unsettled facts and live recovery remain intact or explicitly reconciled |
| I-18 Visible bounded child work | 07, 14 | Attributed events, pause tree and reconstructed child graph agree |
| I-19 Encrypted vault only | 01, 11, 15 | No plaintext/key in observed cloud-bound writes; independent crypto checks |

All invariant cases are hard release gates within the declared support envelope. A high average coding score cannot offset a failure.

## Research experiment coverage and release scoping

The [research experiment matrix](../architecture/othertools.md#202-required-experiment-matrix) defines E01–E20. The current architecture controls which surfaces are in the first release.

| Experiment | First-release implementation/test owner | Later-only extension |
|---|---|---|
| E01 Client behavior parity | 02, 07: interactive/JSONL CLI | 17, 18: API/SDK/editor |
| E02 Instruction precedence | 04, 13: AGENTS.md/skills/hostile inputs | 19: imported instruction formats |
| E03 Lazy capabilities | 04, 13: skill/tool discovery | None required |
| E04 Compaction/handoff | 04, 12 | None required |
| E05 Stale edits | 06, 14: disk and integration races | 18: dirty editor buffers |
| E06 Grants/rewrites | 06, 13 | 19: hook rewrites |
| E07 Platform boundaries | 01, 06, 15: native Windows | 19: other execution hosts |
| E08 Cancellation | 05, 06, 13, 14 | Later client/host adapters |
| E09 Duplicate/disconnected client | 02, 07: internal CLI cursor/idempotency | 17, 18: public attachment |
| E10 Durable-dispatch crashes | 03, 06, 15 | New effectful adapters as added |
| E11 Provider failure/fallback | 05, 12 | None required |
| E12 Concurrent budget admission | 03, 05, 12, 14 | 19: optional observers |
| E13 Memory governance | 08, 09, 10, 15 | None required |
| E14 Storage/index recovery | 03, 08, 09, 15 | None required |
| E15 Worktree integration | 14 | None required |
| E16 Extension failure | 13: MCP and skill lifecycle | 19: hooks/importers |
| E17 Trace inspection/export | 02, 07, 10, 11, 15 | 18: editor presentation parity |
| E18 Install/upgrade | 03, 11, 15: Windows CLI/data | 18, 19: editor/other hosts |
| E19 Routed strategy value | 12, 15 | 19: observer value comparison |
| E20 Memory value | 09, 15 | None required |

## Memory and reuse suites

| Suite | Owning implementation/tests | Required evidence |
|---|---|---|
| M01 | 03, 08 | Files/SQLite governance and canonical parity |
| M02 | 03, 09, 15 | Kill at canonical/index/generation publication barriers |
| M03 | 01, 09, 15 | Actual CPU/RAM/mapped-memory/disk/start/query measurements |
| M04 | 09, 15 | Exact symbols, semantic recall, narrow scope and exhaustive vector oracle |
| M05 | 08, 09, 10 | Supersession/deletion/revocation during lag |
| M06 | 09, 11 | Tokenizer/model/dimension/library rebuild and compatible handover |
| M07 | 03, 09, 10, 15 | Canonical versus derived corruption and resource failure |
| M08 | 01, 11, 15 | Consistent encrypted backup/restore during activity |
| R01 | 01, 02, 07; later 17, 18 | Adapted lifecycle/CLI now; public schema/client behavior later |
| R02 | 01, 04, 06, 14; later 18 | Prepared parsing/context/file conflicts; dirty buffers later |
| R03 | 01, 06, 13 | Effective VCP policy distinct from upstream matches |
| R04 | 01, 06, 15; later 19 | Actual Windows execution/packaging; other hosts later |
| R05 | 01, 05, 06, 12 | Normalized tools/provider/scheduler with budget and effect discipline |
| R06 | 01, 13; later 19 | Skills/MCP now; hook/import semantics later |
| R07 | 02, 04, 08, 12 | Context continuity, full capture, local memory and charged helpers |
| R08 | 01, 15 and every selected-component change | Pinned source/notices, retained tests and update rehearsal |

## Owner acceptance ownership

Detailed setups, actions and assertions are in [segment 16](16-test-fixtures-and-acceptance.md); [segment 15](15-integration-and-release.md) owns final packaged-release execution.

| Suite | Primary feature segments | Final evidence |
|---|---|---|
| U01 Analysis | 04, 05, 08, 09, 13 | Architecture explanation, accurate source references and scoped memory |
| U02 Review | 05, 13, 14 | Seeded finding rubric, unchanged review workspace and visible child trace |
| U03 Generation | 04, 05, 06, 12, 13, 14 | Architecture fit, preserved user changes and tested integrated result |
| U04 Encrypted handoff | 03, 10, 11 | Two Windows environments, independent keys, ciphertext-only vault and restored state |
| U05 History/pruning | 02, 07, 10 | Full artifacts, date/filter semantics, no default deletion and protected refs |
| U06 Pause/resume | 03, 05, 06, 07, 14 | Actual process/dispatch observations and no duplicate effects |
| U07 Routing/optimization | 03, 04, 12 | Explained groups, bounded optional advice, all evaluator costs, adaptive questions and reversible selected policy; P8 rechecks real delegation |
| U08 Skills/MCP | 04, 06, 13 | Declared language/toolset coverage and schema/authority/cancel correctness |
| U09 Local memory compute | 01, 09, 15 | Actual CPU inference/retrieval, no remote embedding and measured resources |

## Maintaining the plan

When an architecture item changes, update its single owning task section, this dependency/evidence row and affected test mappings. Add a new ID for new scope instead of reusing a completed ID for unrelated behavior. Preserve confirmed Windows/CLI scope and local-plaintext/encrypted-cloud requirements. Follow [the code layout](code-layout.md) for repository paths and update affected segments when the source map changes. The layout and community files are supporting guidance, not additional product tasks or evidence of completion. Check local links, unique ownership, dependency existence/acyclicity and complete first-release closure after plan edits.

## Design and decision traceability

The supporting documents below refine existing tasks. Their references to later integration checks do not add implicit dependencies. All 20 ADRs are available from the [decision index](../adr/README.md); their engineering choices remain pending unless evidence explicitly qualifies them.

| Owning segments | Shared implementation contract | Decisions |
|---|---|---|
| 00, 01 | [Task workflow](../development/implementation-workflow.md), [source qualification](../development/upstream-qualification.md) | ADR-001, 003, 008, 013, 015, 019 |
| 02, 03, 06, 07 | [Engine/execution design](../architecture/engine-execution-design.md) | ADR-001–005, 009, 016 |
| 04, 05 | [Context/provider design](../architecture/context-provider-design.md) | ADR-006, 009 |
| 08, 09, 10 | [Memory/retrieval design](../architecture/memory-retrieval-design.md) | ADR-008, 016 |
| 03, 10, 11 | [Storage/portability design](../architecture/storage-portability-design.md) | ADR-003, 015, 019 |
| 12, 13, 14 | [Routing/extensions design](../architecture/routing-extensions-design.md) | ADR-006, 007, 010, 011, 017 |
| 12, 15, 16 | [Bounded decision design](../architecture/decision-evaluation-design.md), including [adoption mapping](../architecture/decision-evaluation-design.md#adoption-map) | [ADR-020](../adr/020-bounded-semantic-decisions.md) |
| 15, 16 | [Qualification/release design](../architecture/qualification-release-design.md) | ADR-012, 018 |
| 17, 18, 19 | [Deferred clients design](../architecture/deferred-clients-design.md) and [extensions design](../architecture/routing-extensions-design.md) | ADR-002, 011, 012, 014 |

Explicit pause without closing belongs to existing P2-07, P3-01/02/04 and P7-06 behavior, with U06 evidence and P8 requalification. It adds no new task ID. The required observation is pause/inspect/resume in the same live CLI with no new root/descendant dispatch after the stop boundary, alongside existing close/reopen cases.

The [Jev exploration](../architecture/exploring-jev.md) contributes a vendor-neutral
bounded-advice pattern within existing P6-02/03/05 and P6-04 qualification. It adds
no task IDs or prerequisite edges: the ledger remains 68 tasks, with 56 in the
first-release closure. Earlier P2/P5 consumers retain their deterministic paths;
memory governance, local retrieval, authority and completion remain independent
of optional remote advice. P6-02 qualifies actual Jev through OpenRouter as the
preferred specialized candidate and a conventional OpenRouter LLM as a comparator
and explicitly permitted fallback; P6-04 compares both against deterministic rules.
The [Jev gateway contract](../architecture/decision-evaluation-design.md#jev-through-openrouter-qualification)
requires endpoint, schema, native probability and usage qualification, including
outage/model-drift behavior. P8 rechecks enabled purposes with actual delegation
and packaged behavior. Direct TypeSafe access/credentials, LangChain or a
Python/JavaScript decision runtime, new local inference dependencies and P10-03
observer agents are not introduced by this plan change.

## Mechanical graph validation procedure

1. Parse only the work-item ownership table into records keyed by full task ID. Expand shorthand within each dependency cell: `P1-01/03` means `P1-01, P1-03`; `P0-02…05` includes all four IDs. Reset phase context when a new explicit phase appears.
2. Reject duplicate/missing IDs, unknown dependencies and self-dependencies. Compare the exact dependency set against the architecture work-item tables, not just task counts.
3. Scan task headings in owner segments and require exactly one `## Pn-nn` heading per ledger ID. Resolve the owner link and anchor to that heading. Supporting design references are not implementation owners.
4. Run a topological sort or depth-first cycle check over prerequisites. Starting at P8-05, include the task itself and recursively collect prerequisites; require exactly all 56 non-P4/P9/P10 tasks and no deferred task.
5. Check FR-01–17 and I-01–19 coverage, E01–20/M01–08/R01–08/U01–09 mappings and first-release scoping. E01/R01 public-client variants and E05/R02 editor variants remain later-only.
6. Check relative links/anchors, navigation entries, proposed path ownership and whitespace. Review new files as well as tracked diff; `git diff --check` alone does not inspect untracked design documents.

Report what this validation proves: consistent documentation ownership, links and graph. It does not prove source implementation, runtime checks, legal review or owner acceptance. Preserve all task states as planned until actual implementation evidence exists.

## Recording readiness and completion

Use the canonical state vocabulary `planned`, `in_progress`, `blocked`, `implemented_unverified` and `complete` (the table displays the initial Planned label). Attach evidence IDs and immutable source/fixture/package identities when updating a row. Each failed or not-run prerequisite remains visible; a whole-file completion marker cannot replace per-task readiness.

For a completed task retain its actual commands/outcomes, supported variants, applicable ADR selection, current evidence and operational limitations. A later relevant change can invalidate evidence without erasing the old result. Record which consumer contracts need requalification and why; do not reset acknowledged history to make a new test pass.

P8-05 joins task state, dependency readiness, FR/I coverage, all U suites and factual owner sign-off for the final package. A planning document, source directory, passing mocked path or undocumented manual run cannot satisfy that join.
