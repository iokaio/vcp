# 15 — Integrated qualification and native Windows release

Status: planned. Owns P5-08 and P8-01 through P8-06. This file contains two stages: memory qualification before P6-04, then final integration after all required CLI features. Do not require the entire file to finish before routing development.

## Code and evidence organization

Implement reusable evaluation runners/graders under `evals/`, fault campaigns under `tests/recovery/`, actual Windows process/install cases under `tests/platform/`, and packaging under proposed `scripts/package.ps1` plus selected workspace packaging configuration. Keep private owner inputs and full run artifacts outside version control; commit synthetic fixtures and redacted evidence summaries only.

Add CI jobs incrementally for formatting/build/contracts, backend parity, Windows process/recovery, real local embedding, deterministic acceptance and packaging. Jobs needing assets or an interactive console state prerequisites explicitly. Paid OpenRouter evaluation runs are separated from ordinary PR checks and require a configured budget.

## P5-08 — Integrated memory acceptance

After P5-01–07/P5-09/P5-10, exercise canonical memory, search, pruning and encrypted restore together. Compare no semantic memory, versioned Markdown recall and governed memory on the same held-out source/question/task fixtures, model configuration and total budget where applicable.

Measure useful sourced recall, stale/unsupported passages, scope violations, task outcome, local resource cost, prompt overhead and model-assisted maintenance spend. Include failed/abandoned runs and index lag. A better recall score cannot excuse restricted-content retrieval or lost acknowledged records.

Run M01–M08, E13/E14/E20 and U04/U05/U09 subsystem cases. Deliver immutable report/fixture/model revisions and honest limitations for P6-04. This stage does not need final routing/delegation implementations; their later integration is rechecked below.

## P8-01 — Native Windows support matrix

Requires portable CLI, qualified routing and bundled skill/MCP/delegation paths. Declare supported Windows versions, architecture, filesystem/shell/terminal, CPU/RAM, local inference provisioning and compiler dependencies based on tests.

Exercise clean user profile, Unicode/spaces, locks/junctions, terminal/PTY and process trees, missing toolchains, local model setup, install locations and policy enforcement. Report actual capability limitations; Windows running under a test VM is acceptable when identified, while WSL tests are not native Windows proof. Record U09 resources on a CPU-only target and revisit provisional architecture performance targets with measured distributions.

## P8-02 — Recovery and portability campaign

Combine real process termination with deterministic write/dispatch barriers. Cover root/child model and tool activity, journal/SQLite writes, artifact finalization, prune tombstones/cleanup, generation publication, encrypted vault copy and restore activation.

For each crash point, record whether work was acknowledged, actual external effects, post-reopen canonical records, ledger and search visibility. Verify no repeated non-idempotent action, lost acknowledged record, resurrected deletion or plaintext publication. Include disk exhaustion, corrupt canonical versus derived data, locked files, interrupted cloud hydration and offline divergence. E10/E14/M02/M07/M08/U04–U06 apply.

## P8-03 — Full-history and encryption review

Audit full capture versus prompt/UI tails using oversized synthetic outputs and child transcripts. Check provider/recovery-key exclusion at request, logging, inspector, bug-report and export boundaries. Exercise 30-day notices, filtered purge and older-cloud-backup disclosure.

Observe sync-folder files through success and failure, independently decrypt with developer-held material and test wrong key, tampering, rotation and approved-writer checks. Local working files remain usable plaintext. A provider's transport or storage encryption cannot count as VCP client-side encryption evidence. Recheck U04/U05/I-19 on packaged binaries, not only libraries.

## P8-04 — Windows distribution

1. Package the native CLI/engine/worker, selected runtime dependencies, built-in skill catalog, notices and provenance manifest. Pin embedding assets or provide explicit digest-verified provisioning under their terms.
2. Implement clean install/start, data-root selection, vault/key setup diagnostics, upgrade and uninstall behavior. Preserve existing repositories, workspace IDs, history, local keys and vault settings; uninstall cannot silently erase them.
3. Version binary/store/index/config compatibility. Upgrade uses recoverable migration; binary rollback is allowed only with compatible state or explicit validated restore.
4. Provide checksums and the applicable signing process; document supported artifact/install formats without implying a signing certificate already exists.

Test install on a clean Windows environment, missing dependency/model, path change, locked executable, interrupted upgrade, unsupported data version, old-binary rollback and encrypted recovery on a second machine. Use the packaged artifact in smoke tests; building from a developer checkout is insufficient.

## P8-06 — Upstream maintenance rehearsal

Use the pinned component manifest to select a representative Codex/Gemini/Munarium update. Review selected code/dependency changes, reapply attributed ports/patches, rebuild and rerun affected upstream plus VCP R suites. Track patch size, failed assumptions and update effort.

Check source/licenses/notices for shipped code, models and native assets, including crypto dependencies. Record exact build inputs with the package. Do not auto-update vendored code at runtime or preserve a green test by silently changing upstream expected behavior.

## P8-05 — Owner acceptance and release evaluation

Run all U01–U09 with the final Windows artifact and predeclared owner task/threshold definitions from segment 16. Owner work must demonstrate architecture-aware analysis, defect review and generation fitting existing code organization, with routing, local memory, MCP/skills, visible children, optimization, history controls and encrypted handoff functioning together.

Complete a release evidence table with fixture/source/artifact revisions, pass/fail/not-run, quality/cost/latency, exact limitations and links to traces. No first-release requirement may be skipped because the average score is high. Check all 56 first-release work items and FR/I mappings in [traceability](20-traceability.md).

Prepare a reviewable release candidate, installation/operations documentation, support matrix and known issues. Owner acceptance is the final product gate. Actual publishing follows the authorization for that implementation session; this plan does not publish software.

## Release blockers

Unauthorized dispatch, silent user-edit overwrite, lost acknowledged records in the declared fault envelope, unaccounted helper calls, purged/restricted context reuse, false completion, hidden child work, plaintext cloud-bound data or unrecoverable handoff with a valid developer key block release. Missing measurements or skipped required environments remain not-run, never implied passes.
