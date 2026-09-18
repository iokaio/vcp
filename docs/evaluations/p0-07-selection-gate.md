# P0-07 selection gate and effect classification

Status: `implemented_unverified`, pending hosted checks of the schema 2 change.
P0-01 is complete. The baseline's hosted build/reconstruction and helper traces
passed before this change; this change adds the final explicit source
classifications and consistency checks required by P0-07.

This report consolidates the immutable-source selection gate. It does not
qualify VCP execution, local corpus retrieval, operating-system isolation,
durable pause, encrypted portability or an installable release.

## Acceptance map

| P0-07 obligation | Evidence and qualified boundary |
|---|---|
| Immutable candidates and native baselines | [Original native runs](p0-07-native-candidates.md) record unmodified Codex CLI build, selected policy/patch tests, Gemini comparison tests and Munarium kernel/store tests; [datastore follow-up](p0-07-munarium-datastore.md) covers the selected Tantivy/DiskANN libraries |
| Selected source, dependency and rights records | [Codex selection](../../src/third_party/components/codex.md), [Munarium selection](../../src/third_party/components/munarium.md), immutable manifests, ordered patches, file digests, retained notices and Cargo lock; [Gemini](../../src/third_party/components/gemini-cli.md) remains an external comparison candidate with an immutable Node lock, not a production extraction |
| Ordinary committed source and independent reconstruction | [Codex import](p0-07-codex-import.md) and [Munarium import](p0-07-munarium-import.md); normal builds consume committed source, while explicit maintenance reconstructs and compares the same files |
| Selected embedding assets and runtime | [CPU embedding evidence](p0-07-local-embeddings.md): pinned Candle/Tokenizers, ten verified external MiniLM files, retained software terms/model declaration, independent numerical reference; no implicit download or remote inference fallback |
| Another Windows environment | [Hosted qualification](p0-07-hosted-windows.md), followed by the embedding and helper-trace PR runs, builds committed inputs and independently reconstructs both selections on `win8core` in `wingroup` |
| Classify selected modules and identify replacements | [Schema 2 catalog](../../src/third_party/components/codex-boundaries.json) assigns all 158 packages to 23 explicitly effectful conservative envelopes with primary owners; 43 reviewed named entries refine these into one pure, three read-only and 39 effectful entries |
| Expose helper, credential and maintenance paths | [Classified effects](../development/upstream-effect-classes.md) and [helper map](../development/helper-effect-traces.md) include CLI input/shutdown, retry, review, both compaction routes, memory helpers, realtime, ambient credentials, telemetry, update probes, daemon updates and rollout maintenance |

The module classification is conservative. Unreviewed exports retain effectful
treatment; a narrow pure/read-only entry does not grant its package permission
to act. Matching source anchors and consistent metadata are not a formal call
graph or a security boundary. Unexecuted feature combinations remain unqualified.

## Checks for the classification change

The checker now rejects missing module ceilings/owners, a ceiling incompatible
with declared capabilities, a named entry above its module ceiling, missing or
unknown classes/effects, duplicate effects, pure entries with I/O and read-only
entries with mutation. Existing pin, package ownership, path containment,
source-symbol and task-owner checks remain in place.

Seven additional source entries expose release-only update discovery, daemon
installation/restart, rollout locking/compression, the live core facade,
configuration telemetry and rendering diagnostics. `codex-core-api` moves to
controller ownership and `codex-features` to configuration. Imported source,
patches, dependency pins and runtime behavior are unchanged.

`node scripts/upstream/check-boundaries.cjs` passed with 158 packages, 23 groups,
43 entries and the classification counts above. The focused four-test boundary
suite passed. `pwsh -NoProfile -File scripts/test.ps1 -Suite fast` then passed
all eight registered cases and 56 regressions, exit 0, including both source
inventories and documentation/ledger checks. Evidence:
`artifacts/tests/8777bca1-0f21-4502-aa95-7a1148538fd8/manifest.json`,
September 18, 2026, 03:49:21–03:49:48 UTC, Windows 10.0.26200, Node 24.10.0,
Git 2.51.0.windows.1. The source base was
`057acc893e51fb68e57e6deebd7cd6cff0c8b80f` with this change uncommitted.
`git diff --check` passed. Hosted checks of the published head remain pending.

The prior [helper-trace hosted run](p0-07-helper-traces.md#scope-and-remaining-work)
passed on the unchanged imported source and dependency pins, including independent
reconstruction and all nine native traces. It is baseline evidence, not a claim
that the new classification checker has already passed hosted CI.

## Subsequent task boundaries

P0-02 owns scoped real-corpus ingestion, lexical/ANN recall, process reopen,
asset failures, observed network denial and resource measurements. P0-03 owns
the retained controller's lifecycle, including `/pause` with the CLI still open,
children, owner loss and deliberate resume. P0-05 owns actual Windows process
and access enforcement. P0-08 owns injected model/budget/store adapters, removal
of implicit credential/telemetry/update routes and a common product toolchain.
P0-09 owns attributed Gemini ports and their complete imported dependency/rights
closure. P0-04 chooses cryptographic dependencies and qualifies writer trust;
none is selected by this source gate. P0-06 can close only after all its own
prerequisites pass.

Earlier reports retain their historical in-progress states and failed runs.
This consolidated map must not turn their unrun product checks into passes.
