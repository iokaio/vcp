# P5-08 integrated memory qualification

The September 20, 2026 campaign joins canonical recall, bounded recent-memory
visibility, retention and authenticated cross-backend restore. Dependencies
P5-01–07 and P5-09/10 were complete at source `8b9cd7f`; the implementation
and new fixtures are delivered together with this report.

The integrated review found that retrieval omitted acknowledged records newer
than its published generation. Retrieval now consumes the existing bounded
authorized overlay, ranks lexical token matches deterministically and identifies
overlay ranks separately from Tantivy/vector ranks. The fusion identity advances
to `rrf-equal-k60-id-ascending-overlay-terms/2`. Overlay limits and lexical-only
freshness remain visible; recent lexical recall does not claim fresh vectors or
satisfaction of an indexed minimum sequence. Current task, source, policy and
deletion checks still run before returning passages and again before dispatch.

## Frozen comparison

The [fixture and procedure](../../src/evals/memory/README.md) compare empty recall,
versioned Markdown current-head recall and production governed recall. Separate
source history and held-out question files are hashed into every report. The
same six evidence questions, task scopes and limits run on Files and SQLite,
against both a lagging generation and a current generation: 72 total attempts.
No model, provider or embedding inference runs in this comparison; maintenance
and extraction model spend are zero. This is evidence-task qualification, not a
measurement of coding-model answer quality.

| Strategy | Attempts | Complete evidence tasks | Relevant sources found | Stale / forbidden / unsupported |
|---|---:|---:|---:|---:|
| No semantic memory | 24 | 12 | 0 / 12 | 0 / 0 / 0 |
| Versioned Markdown | 24 | 24 | 12 / 12 | 0 / 0 / 0 |
| Governed memory | 24 | 24 | 12 / 12 | 0 / 0 / 0 |

The empty strategy passes the correctly unanswerable cases and fails required
recall. Equal Markdown/governed scores do not establish a quality advantage.
All attempts, exact evidence identities, prompt byte counts, elapsed query time,
ingestion/build time and retained bytes are in the local report. Missing RSS,
embedding time and restore-readiness measurements are explicit nulls. The
six-question preference corpus is too small for production quality conclusions.

## Integrated boundaries

`vcp-memory/tests/integrated_restore.rs` runs Files→SQLite and SQLite→Files,
with and without recall exclusion. It independently checks:

- Initial sourced recall and invalidation of an already returned fence after exclusion.
- An old pinned generation cannot expose the excluded passage.
- Restart, encrypted capture, staged-acquisition reopen, authentication and import.
- Byte-identical retained evidence and original acknowledged transaction receipts/events.
- Destination authority revision changes and stale source access is rejected.
- Explicit unavailable search before rebuild, then sourced recall or persistent exclusion.
- Narrow task access remains empty after restore; the original root is unchanged.

Recall exclusion deliberately retains raw evidence. Physical erasure and backup
obligations are covered separately by the existing purge/portable-retention tests.
The integration uses real cryptography and two native backend implementations on
one machine. Actual two-Windows OneDrive handoff remains the separately recorded
[U04 campaign](../development/p3-onedrive-qualification.md), not an inference from
this local test.

## Reproduction and evidence

Use the established native Windows compiler environment and cached dependencies:

```text
cargo +stable test --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline -j4 -p vcp-memory --tests --example memory_comparison
pwsh -NoProfile -File scripts/evals/memory-comparison.ps1 -Jobs 4
cargo +stable clippy --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline -j4 -p vcp-memory --lib --test retrieval --test integrated_restore --example memory_comparison
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

The memory regression log is `artifacts/p5-08-memory-tests.log`: 95 tests passed,
with two opt-in real-model cases excluded from that invocation. Both opt-in
cases then passed explicitly: real CPU batches/cache/exact-vector oracle, and
16 process-kill publication/recovery cases across both backends. Their log is
`artifacts/p5-08-native-model.log`. Changed-source formatting and diff checks
passed. Clippy passed with unchanged dependency warnings and no warnings in the
changed memory code or example (`artifacts/p5-08-clippy-final.log`).

The final comparison manifest, including before/after identities for 387 relevant
source/configuration files and an unchanged-source check, is
`artifacts/p5-08-comparison/b94cba7a-c5a8-42b8-ba1d-e324c67207a2/manifest.json`.
The earlier comparison is also retained under
`artifacts/p5-08-comparison/d5fe63a8-a07f-4458-bb1c-53f5df3e4c48`.
Seven initial repository checks passed and one provenance check failed because
a temporary compiler cache was created inside vendored source. Moving that
generated cache to `artifacts/` restored all eight checks; no source validation
was relaxed. Initial test compilation also caught and corrected invalid `Access`
cloning and a test reference comparison before execution.

| Acceptance boundary | Evidence |
|---|---|
| E13 / M01 governed evidence and all six classes | Current governed, extraction, extractor and ingestion-recovery tests; [P5-01 contract](../development/p5-governed-memory.md) |
| E14 / M02 / M06 publication and recovery | Current publication tests, explicit real-model process-kill campaign, cross-backend encrypted restore test |
| E20 / M04 useful scoped recall | Frozen comparison, current retrieval/filter/lexical regressions, hybrid campaign |
| M03 / U09 local semantic retrieval | Explicit CPU/vector oracle test and denied-network hybrid campaign; [resource envelope](../development/p5-vectors.md) |
| M05 / M07 / U05 retention | Current governed purge, portable-retention, retention-policy and pinned-view/restore exclusion tests; [native retention evidence](../development/p5-retention.md) |
| M08 / U04 encrypted handoff | Current authenticated cross-backend integration plus [actual two-Windows U04](../development/p3-onedrive-qualification.md) and [retained-vector restore qualification](../development/p3-portability-qualification.md) |

The unchanged two-workspace hybrid corpus also passed with the new retrieval
implementation: 126 query repetitions, 42 scope checks and four generation
builds. All lexical/vector/fused recall and exhaustive-vector agreement checks
passed. Actual MiniLM inference ran in the established zero-capability
AppContainer; the trusted canonical process consumed validated local vectors.
Missing/corrupt assets and four tampered handoff bundles were rejected. The
manifest is `artifacts/p5-08-hybrid/1f707b9e-7da7-43e1-b658-dac00bf2b1e5/manifest.json`.
Use the [P5-06 native command](../development/p5-retrieval.md#native-hybrid-campaign)
with the rebuilt `hybrid_quality` executable. This preserves the original pinned
corpus/model and independently checks cross-workspace restrictions; it does not
establish a semantic advantage over lexical search on this small fixture.

The qualification covers bounded deterministic memory behavior. Routing quality,
actual delegated coding outcomes, packaged release acceptance and owner sign-off
remain P6/P7/P8 responsibilities. Process-kill evidence does not establish hardware
power-loss durability. No paid evaluation or release publication is implied.
