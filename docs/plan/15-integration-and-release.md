# 15 — Integrated qualification and native Windows release

Status: P5-08 complete with [integrated native memory evidence](../evaluations/p5-08-integrated-memory.md). P8-01 through P8-04 now have an executable matrix, an unsigned Windows distribution candidate and [partial native qualification](../evaluations/p8-native-qualification-2026-09-22.md); clean-machine, second-machine recovery and full integrated acceptance remain open. On September 22 the owner directed this continuation to skip machine handoff and validate on the current workstation; independent-machine and clean-OS evidence remain explicitly not run. P8-06 is complete with the [bounded upstream maintenance rehearsal](../evaluations/p8-upstream-maintenance-2026-09-22.md); P8-05 human acceptance remains open. This file contains two stages: memory qualification before P6-04, then final integration after all required CLI features. Do not require the entire file to finish before routing development.

Use the [qualification and release design](../architecture/qualification-release-design.md) for executable harness, fault-oracle, package and evidence contracts. [ADR-012](../adr/012-clients-and-distribution.md) owns client/platform sequencing, [ADR-018](../adr/018-release-acceptance.md) records the complete usable-release gate, and [ADR-019](../adr/019-cloud-encryption-and-keys.md) retains the still-unqualified crypto choices. Architecture [section 20](../architecture/vcp-what.md#20-testing-evaluation-and-performance) supplies the quality and invariant authority.

The [current-machine qualification report](../evaluations/p8-local-qualification-2026-09-22.md) records the later exact-package, encryption, local restore and native follow-up evidence, including failures and bounded-run limitations.

## Code and evidence organization

Implement evaluation orchestration under `scripts/evals/`, reusable graders and task definitions under `src/evals/`, fault campaigns under `src/tests/recovery/`, actual Windows process/install cases under `src/tests/platform/`, and packaging under proposed `scripts/package.ps1` plus selected workspace packaging configuration. Keep private owner inputs and full run artifacts outside version control; commit synthetic fixtures under `src/` and redacted evidence summaries under `docs/evaluations/` only, following [the code layout](code-layout.md).

Add CI jobs incrementally for formatting/build/contracts, backend parity, Windows process/recovery, real local embedding, deterministic acceptance and packaging. Jobs needing assets or an interactive console state prerequisites explicitly. Paid OpenRouter evaluation runs are separated from ordinary PR checks and require a configured budget.

Current cost-control policy: routine PR/main CI runs the fast delivery suite on
standard Ubuntu; full native Windows qualification is available only through
manual dispatch on a standard runner. See the [qualification procedure](../development/codex-source.md#native-windows-ci)
for invocation and reversal. This changes when expensive checks run, not the
evidence required to complete product tasks or qualify a release. A green routine
check or skipped native job does not satisfy native acceptance.

## P5-08 — Integrated memory acceptance

**Planned follow-up:** [M5](21-markov-integration.md#m5--context-co-change-retrieval-and-compaction)
adds map/PageRank/co-change and optional memory-graph comparison arms to the frozen
context/retrieval task set. Re-run affected recall, scope, retention and resource
cases before P6-04 uses changed context defaults. Existing completed evidence
qualifies the original pipeline, not these candidate extensions.

After P5-01–07/P5-09/P5-10, exercise canonical memory, search, pruning and encrypted restore together. Compare no semantic memory, versioned Markdown recall and governed memory on the same held-out source/question/task fixtures, model configuration and total budget where applicable.

Measure useful sourced recall, stale/unsupported passages, scope violations, task outcome, local resource cost, prompt overhead and model-assisted maintenance spend. Include failed/abandoned runs and index lag. A better recall score cannot excuse restricted-content retrieval or lost acknowledged records.

Run M01–M08, E13/E14/E20 and U04/U05/U09 subsystem cases. Deliver immutable report/fixture/model revisions and honest limitations for P6-04. This stage does not need final routing/delegation implementations; their later integration is rechecked below.

### Construct the memory comparison

Freeze corpus/task revisions, authorized scopes, source applicability and independent relevant-result labels before tuning. Reset each strategy to its own declared initial state, then replay the same observation history. Keep the held-out questions outside extraction/tuning inputs. Record whether each answer used lexical, vector, recent overlay or canonical fallback evidence.

Run the three strategies with matched model/provider, tool access and task cap when billable tasks are used; include extraction and maintenance calls in total cost. Measure retained bytes, embedding/build time, query time and restore readiness separately from model output quality. An unknown or pruned evidence source is not an incorrect citation to silently replace.

The report must include all attempted queries/tasks, sourced recall and stale/forbidden passages, costs including unsuccessful work, hardware/corpus identity and failure cases. A scope violation or lost canonical record fails the gate regardless of aggregate recall. P6-04 consumes this dated result and reruns affected cases after relevant memory changes.

## P8-01 — Native Windows support matrix

Include the selected [Cursor](22-cursor-improvements.md) and
[Claude Code](23-claudecode-improvements.md) refinements in the same packaged
matrix: bounded search/read coverage, declared long foreground checks, Windows
output decoding, helper setup diagnostics and current-result review. Test only
advertised shell/toolchain combinations, preserve direct-profile verification
requirements, and record unavailable combinations as not-run. These are
qualification rows for P7 deliverables, not a second implementation backlog.

Include [M9's enabled statistical consumers](21-markov-integration.md#m8m10--reuse-and-qualification-campaigns)
in packaged Windows and actual-delegation qualification, with both-store
prune/restore/replay coverage in P8-02 and the final enabled/rejected/deferred
inventory at P8-05. Seeded synthetic tests supplement existing fault fixtures;
they do not establish live cost savings or replace owner acceptance.

Requires portable CLI, qualified routing and bundled skill/MCP/delegation paths. Declare supported Windows versions, architecture, filesystem/shell/terminal, CPU/RAM, local inference provisioning and compiler dependencies based on tests.

Exercise clean user profile, Unicode/spaces, locks/junctions, terminal/PTY and process trees, missing toolchains, local model setup, install locations and policy enforcement. Report actual capability limitations; Windows running under a test VM is acceptable when identified, while WSL tests are not native Windows proof. Record U09 resources on a CPU-only target and revisit provisional architecture performance targets with measured distributions.

### Turn support claims into matrix rows

Key a row by OS build, CPU architecture/features, filesystem, shell/terminal, install method, engine package digest and embedding/runtime identity. Each claimed capability references an actual case: argument preservation, path/link handling, process-tree stop, restricted access, local inference and reopening indexes. Avoid declaring an entire platform supported from one successful build.

For each supported environment run explicit `/pause`, inspect state while the CLI remains open and `/resume` in the same process, then separately test console closure. Record acknowledgement/stop latency separately from worker termination. Missing interactive-console coverage remains not-run even when headless cases pass. Qualification may narrow a measured hardware envelope but cannot replace native Windows with WSL.

## P8-02 — Recovery and portability campaign

The [local boundary audit](../evaluations/p8-local-recovery-boundary-map-2026-09-22.md)
maps retained receipts before selecting the
[bounded recovery increment](../evaluations/p8-local-recovery-increment-2026-09-22.md).
The [remaining local recovery increment](../evaluations/p8-local-recovery-followup-2026-09-22.md)
adds forty independently observed process kills, bounded capacity failures,
offline divergence, native/MCP content authority and an interrupted platform
cloud-hydration helper followed by exact-package VCP restore and reopen. Its
finite schedule supplements the existing matrix; physical-volume exhaustion,
interruption while VCP restore itself is active and production-package acceptance
remain open. Machine handoff stays owner-skipped.

Apply [CR-10a / CC-08](22-cursor-improvements.md#cr-10a--qualify-existing-trust-boundaries)
to the integrated native-tool, MCP and child paths. Exercise unsafe outside-root
and execution-configuration writes, junctions, destructive cleanup and ordinary
authorized developer edits. Confirm existing policy and grants first; add no
blanket prompt or denial merely because a path looks sensitive. Include pause or
owner loss during long checks and cleanup, exact owned-path validation, locked
roots, retained recovery references and both-store reopen. Fix any demonstrated
boundary defect before release without changing the intended permission model.

Combine real process termination with deterministic write/dispatch barriers. Cover root/child model and tool activity, journal/SQLite writes, artifact finalization, prune tombstones/cleanup, generation publication, encrypted vault copy and restore activation.

For each crash point, record whether work was acknowledged, actual external effects, post-reopen canonical records, ledger and search visibility. Verify no repeated non-idempotent action, lost acknowledged record, resurrected deletion or plaintext publication. Include disk exhaustion, corrupt canonical versus derived data, locked files, interrupted cloud hydration and offline divergence. E10/E14/M02/M07/M08/U04–U06 apply.

### Execute the fault schedule

Build a matrix of barrier, before/after position, backend, root/child role and repeat seed. Use the [independent supervisor](../architecture/qualification-release-design.md#fault-supervisor-and-acknowledgement-oracle) to confirm the barrier before killing a real process. Preserve supervisor acknowledgements and external-effect markers outside the engine store.

On reopen, compare canonical records and complete artifact references with the acknowledged set, then reconcile uncertain operations without replay. Query during generation lag and after prune/restore to catch delayed disclosure. Test interrupted activation with both old and new roots present; recovery must choose a validated root, not whichever path is newest.

Start with every boundary individually, then combine selected realistic sequences such as pause during child output followed by backup copy interruption. Do not claim exhaustive interleavings. Keep failing seeds/barriers as repeatable regression cases; never repair a recovery run by deleting its data root.

## P8-03 — Full-history and encryption review

The [local history/security follow-up](../evaluations/p8-history-security-followup-2026-09-22.md)
records the sensitive-surface audit, packaged MCP compaction/reopen schedule,
independent cryptographic verification and exact candidate identities. The MCP
schedule exposed a startup defect: acknowledged partial process output was
treated as an unresolved capture. The correction distinguishes verified,
canonically acknowledged partial output from captures that must remain fenced.
Execution status and remaining release limits are recorded in that report and
its [surface map](../evaluations/p8-sensitive-surface-map-2026-09-22.md).

Cover any records/artifacts added by the
[selected workflow refinements](20-traceability.md#developer-workflow-follow-up-readiness):
full process bytes versus decoded previews, finding evidence, worktree ownership
and cleanup receipts. Pruning and restore must preserve live dependencies and
explicit gaps; cleanup of a disposable root cannot silently erase its history.
For [CR-13](22-cursor-improvements.md), qualify existing retained-output and
history inspection plus truthful MCP identity/state across compaction and
reopen. This adds evidence, not a new recall tool or unconditional context part.

Audit full capture versus prompt/UI tails using oversized synthetic outputs and child transcripts. Check provider/recovery-key exclusion at request, logging, inspector, bug-report and export boundaries. Exercise 30-day notices, filtered purge and older-cloud-backup disclosure.

Observe sync-folder files through success and failure, independently decrypt with developer-held material and test wrong key, tampering, rotation and approved-writer checks. Local working files remain usable plaintext. A provider's transport or storage encryption cannot count as VCP client-side encryption evidence. Recheck U04/U05/I-19 on packaged binaries, not only libraries.

### Review captured and exported surfaces

Trace synthetic sensitive markers through provider inputs, tool environment, full artifact capture, terminal rendering, JSONL, inspectors, diagnostic bundles and snapshot staging. Typed credentials/keys must be excluded before serialization; record legitimate redaction/omission categories. Retained ordinary source text is subject to scope and explicit export policy, not an unsupported promise that heuristics detect every secret.

Observe vault objects from creation through failed finalization/copy and verify their encrypted format independently. Review path/junction separation at use time, recipient pins, writer enrollment and key rotation. Reports distinguish locally published, transfer observed and restore verified. Purge reports distinguish active exclusion, physical cleanup and retained external copies; no report may claim universal erasure.

## P8-04 — Windows distribution

1. Package the native CLI/engine/worker, selected runtime dependencies, built-in skill catalog, notices and provenance manifest. Pin embedding assets or provide explicit digest-verified provisioning under their terms.
2. Implement clean install/start, data-root selection, vault/key setup diagnostics, upgrade and uninstall behavior. Preserve existing repositories, workspace IDs, history, local keys and vault settings; uninstall cannot silently erase them.
3. Version binary/store/index/config compatibility. Upgrade uses recoverable migration; binary rollback is allowed only with compatible state or explicit validated restore.
4. Provide checksums and the applicable signing process; document supported artifact/install formats without implying a signing certificate already exists.

Test install on a clean Windows environment, missing dependency/model, path change, locked executable, interrupted upgrade, unsupported data version, old-binary rollback and encrypted recovery on a second machine. Use the packaged artifact in smoke tests; building from a developer checkout is insufficient.

### Assemble and test the exact distribution

Produce the [package inventory](../architecture/qualification-release-design.md#package-and-compatibility-manifest) from selected build outputs, not a broad workspace archive. Check every included path against the expected inventory, notices and asset terms; Git ignore rules do not control packaging. Reject unexpected caches, local stores, test secrets or fixture repositories.

Record the final artifact digest after packaging/signing and install those exact bytes into a clean profile without developer cache dependencies. Exercise first-run diagnosis, local model provisioning, explicit pause/resume, a scripted coding task and encrypted recovery. Test uninstall with distinct sentinel data in user workspace/history/key locations and assert it remains intact.

Migration first prepares a recoverable snapshot, validates the target format in staging and only then changes the active root. Test old binaries against newer state and refuse unsafe rollback. Signing/certificate availability and artifact format are qualified choices; document missing prerequisites without fabricating signed-release evidence.

## P8-06 — Upstream maintenance rehearsal

Status: complete. The [September 22 rehearsal](../evaluations/p8-upstream-maintenance-2026-09-22.md) reconstructed the adjacent immutable Codex revision with all 36 patches unchanged, reviewed the full 72-file delta and authority implications, and ran affected retained and VCP suites. The recorded native-launch dialog finding was fixed and closed by three fresh launch regressions plus the exact both-store verification case unattended. Original failures, manual interventions, maintenance effort and remaining P8 qualification limits are preserved in the report.

Use the pinned component manifest to select a representative Codex/Gemini/Munarium update. Review selected code/dependency changes, reapply attributed ports/patches, rebuild and rerun affected upstream plus VCP R suites. Track patch size, failed assumptions and update effort.

For Codex, follow [ADR-013](../adr/013-upstream-reuse-and-vendoring.md): prepare the new source selection and patches in an isolated directory, compare the reconstruction with the proposed committed tree, and verify the resulting source through the VCP build. Review the copied source, immutable pin, hashes, patch series, and notices together. Ordinary builds consume that committed result without fetching Codex or applying patches; upstream revision changes are explicit maintenance work.

Check source/licenses/notices for shipped code, models and native assets, including crypto dependencies. Record exact build inputs with the package. Do not auto-update vendored code at runtime or preserve a green test by silently changing upstream expected behavior.

### Preserve an auditable update comparison

Choose the representative update and affected contracts before patching. Record old/new immutable selection, dependency closure, conflicts, patch-series changes and elapsed maintenance effort. Reconstruct the proposed new tree independently and run original retained tests plus affected VCP boundary suites on it.

Keep pre-existing upstream failures distinct from new regressions and intentional VCP differences. Inspect effect inventories for newly introduced retry, credential, network or telemetry paths. Update notices and model/native asset inventory with source changes. A smaller patch is useful only if the single controller, authority and accounting contracts still hold.

## P8-05 — Owner acceptance and release evaluation

Preparation is in progress: the [release scorecard](../evaluations/p8-release-scorecard-2026-09-22.md)
joins the 56-task closure, FR/I/U dispositions and 46 passing executable matrix
cases. The [gap reconciliation](../evaluations/p8-acceptance-gaps-2026-09-22.md)
and [owner packet](../evaluations/p8-owner-acceptance-package-2026-09-22.md)
preserve missing final acceptance and the owner-skipped machine handoff. No
owner sign-off or release publication is recorded.

Join the [workflow follow-up ledger](20-traceability.md#developer-workflow-follow-up-readiness)
to the existing release evidence. Selected refinements require implementation
and their stated acceptance; deferred candidates in plans 22/23 are not release
requirements. Demonstrate an ordinary analysis/review/generation flow with useful
defaults, concise progress, actionable setup failures and no repeated approvals
for already authorized work. Record actual developer interventions as well as
quality, time and total root cost. A planning or automated review result cannot
stand in for the owner's release acceptance.

Run all U01–U09 with the final Windows artifact and predeclared owner task/threshold definitions from segment 16. Owner work must demonstrate architecture-aware analysis, defect review and generation fitting existing code organization, with routing, local memory, MCP/skills, visible children, optimization, history controls and encrypted handoff functioning together.

Complete a release evidence table with fixture/source/artifact revisions, pass/fail/not-run, quality/cost/latency, exact limitations and links to traces. No first-release requirement may be skipped because the average score is high. Check all 56 first-release work items and FR/I mappings in [traceability](20-traceability.md).

Prepare a reviewable release candidate, installation/operations documentation, support matrix and known issues. Owner acceptance is the final product gate. Actual publishing follows the authorization for that implementation session; this plan does not publish software.

### Close evidence and owner review

Build the release table by joining the task ledger, FR/I mappings, suite registry and actual run manifests. Each required case/backend/host variant needs current passing evidence or an explicit unresolved blocker. Check that the P8-05 dependency closure still contains exactly the 56 first-release tasks, with no deferred feature treated as required.

Run held-out U01–U03 on the final integrated artifact with all support costs and interventions counted; keep executable graders and human architecture-fit review separate. Run U06 both with the terminal kept open during pause and through close/reopen. Confirm operations instructions match the tested command grammar and recovery path.

Record owner review factually, including which package and scorecard were examined and any unresolved limitations. Do not tick human approval because automation passed. Relevant fixes invalidate the affected evidence and package identity; rerun those cases before requesting publication authorization.

## Release blockers

Unauthorized dispatch, silent user-edit overwrite, lost acknowledged records in the declared fault envelope, unaccounted helper calls, purged/restricted context reuse, false completion, hidden child work, plaintext cloud-bound data or unrecoverable handoff with a valid developer key block release. Missing measurements or skipped required environments remain not-run, never implied passes.
