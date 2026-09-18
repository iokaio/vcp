# P0-02 local corpus and fresh-process index results

Status: this bounded corpus experiment passes on native Windows. P0-02 remains
`in_progress`; governance integration, observed OS network denial and broader
resource/scale measurements remain open. No production memory service is qualified.

## Executed path

The [procedure](../development/local-memory-spike.md) and
[original fixture](../../src/tests/fixtures/local-memory/README.md) connect the
verified CPU MiniLM helper to retained Munarium `ShardWriter`/`OpenShard`, with
Tantivy 0.22.1 and DiskANN 0.56.0. All 24 documents are embedded locally at 384
dimensions with L2 normalization/cosine distance. Each of the two workspaces has
its own artifact; Atlas's obsolete pause version remains stored but is excluded
from the current view before final result limiting.

```powershell
pwsh -NoProfile -File scripts/test-local-memory.ps1 -AssetsRoot <verified-external-model-directory> -TargetRoot artifacts/embedding-target
```

The command passed all eight stages, exit 0, on September 18, 2026,
04:22:40–04:26:34 UTC. Host: Windows 10.0.26200 x64, Rust 1.98.0
(`88d9e12ae`), MSVC 14.50.35717, Node 24.10.0, four build jobs and 12 logical
processors. Source base was `d0cb9870358bb633b36edd0f3f1c792faf50f59c` with this
implementation uncommitted. Evidence:
`artifacts/local-memory/6bef61d7-c715-467e-a21f-0bca6ad30f42/manifest.json`.

Three native unit checks passed: filtering foreign/obsolete candidates before
the result limit, independent cosine ordering/ties, and receipt/workspace
rejection before index access. The release executable SHA-256 was
`a02e387a5a13fb468957c7172ff8aeca48870f6002ab89091dc0a04aabadcc5b`.

The native observer ran five phases: build, reopen, second reopen, wrong receipt,
and corrupted manifest. The last two retain expected nonzero child outcomes;
timeouts and capture/launch failures cannot satisfy their assertions. Both
fresh-process queries returned the same meaningful IDs and recall. Trace evidence
is `trace/4b9780c7-72e8-48cb-890f-4c3366174da5/manifest.json` beneath the run above.

## Relevance and observed measurements

All seven query cases met their predeclared required IDs. No result belonged to
another workspace or an obsolete version. All seven raw three-result ANN queries
matched the independent exhaustive f64 cosine oracle: recall@3 = 1. The expected
semantic result appeared first for pause, budget, encryption, prepared edits and
music. Required symbol results also appeared first. The lexical analyzer can
return additional term matches; these checks do not establish literal-symbol
precision for every identifier syntax.

| Observation | This run |
|---|---|
| Build-process model load | 472 ms |
| Corpus inference, 12 documents per workspace | Atlas 171 ms; Boreal 102 ms |
| Tantivy/DiskANN artifact construction | Atlas 147 ms; Boreal 141 ms |
| First query-process model load | 530 ms |
| Artifact reopen | Atlas 9 ms; Boreal 8 ms |
| Individual query inference | 6,403–11,125 microseconds |
| Combined lexical/vector lookup and candidate filtering | 68–228 microseconds |
| Final two artifacts plus receipt | 76,553 bytes |

These are one-run small-fixture observations. They are not cold OS-cache,
peak-memory/disk, throughput-at-scale, minimum-hardware or production-latency
claims. The exact oracle is separate from semantic relevance; agreement with an
ANN index alone cannot establish a useful model.

## Source and dependency evidence

Corpus SHA-256:
`7984e2aaee47008a1cfdf85d4a107c0bb3f3d4b3184e652733834e6b18a4a21f`.
The [asset specification](../../src/third_party/components/minilm-assets.json)
remains `ba5fd8384a05e519fb44b9aa233e94cf82996dbbc15873bdf690cb68465f5c11`.
The [209-package native graph](../../src/third_party/components/local-memory-dependencies.json)
contains only the previously reviewed embedding/datastore package identities plus
the original executable. Its check requires real local engines and rejects the
identified remote provider, server, database and GPU packages. Dependency
inspection is not an operating-system network boundary.

The shared workspace now has 159 packages; its lock has 1,566 entries, preserving
every one of the previous 1,565 identities/checksums. Only `vcp-memory-spike` is
added. Lock SHA-256:
`203f535c75fd7b82d0a3992722891c8b0223cd192797bcec9bd20bedaabd16bd`.
The fourth ordered Codex workspace patch has SHA-256
`7f092309a906e917cdae96428f6c07d451fc7ab3a47be5280089d5b886abf305`.
Independent reconstruction matched all 7,937 selected files with result digest
`57df61c646b295bd7d92e22c4a974238541d058a4f0658af20f2b5e00d44d6fb`.
Records are `artifacts/upstream/codex-corpus-reconstructed.json` and its distinct
reconstruction directory. Imported Rust source and dependency pins are unchanged.

Integration exposed a static discovery defect: equivalent relative paths from
an external crate caused one real Cargo package to be counted twice. Discovery
now keys by resolved directory identity after containment validation. Its new
regression preserves rejection of escaping links and distinct packages with
duplicate names. The final catalog check passes 159 packages, 23 groups and 45
classified source entries.

Cargo's actual workspace membership independently matched the checker: 159
packages from `cargo +1.98.0 metadata --locked --offline --no-deps --format-version 1`.
The final `pwsh -NoProfile -File scripts/test.ps1 -Suite fast` run passed all
eight cases and 62 regressions, exit 0, including source verification, the new
observer/dependency checks and path-alias regression. Evidence:
`artifacts/tests/a5c968fa-4390-4545-8d47-6f1de9bd8262/manifest.json`.
Repository link/task checks and `git diff --check` passed after removing one
trailing blank line found in review. No assertion or relevance expectation was
weakened in response to observed results.

## Remaining qualification

The canonical view here is a fixed fixture, not a durable VCP governance store.
It does not exercise live deletion, policy changes during queries, crash recovery,
claim acceptance or task authority. The bounded candidate set is fetched before
filtering; efficient production backfill remains later work. Observed OS network
denial, missing/corrupt model integration and declared resource/corpus scales are
still required before P0-02 can close. `/pause` remains P0-03 lifecycle work.

Hosted CI for this change is pending; prior upstream CI is recorded separately
in the [completed P0-07 gate](p0-07-selection-gate.md).
