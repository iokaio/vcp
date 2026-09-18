# P0-07 — Verified local CPU embedding baseline

Status: the shared-workspace helper passed native asset/source verification,
three library regressions, dependency validation and four real model checks.
P0-07 remains `in_progress`. This result does not complete P0-02, VCP memory/index
integration, OS network enforcement, pause/resume or release packaging.

## Source, assets and independent reference

The [implementation guide](../development/local-embeddings.md) records the
file-only loader, limits, commands and caller responsibilities. The original
VCP crate uses Candle 0.11.0 and Tokenizers 0.22.2 in the existing Cargo workspace.
The published Candle source revision is
`31f35b147389700ed2a178ee66a91c3cc25cc80d`; the initially inspected newer source
head is recorded separately. No upstream engine is added.

The external model is all-MiniLM-L6-v2 revision
`1110a243fdf4706b3f48f1d95db1a4f5529b4d41`. Its ten selected files total
91,578,299 bytes, including the 90,868,376-byte safetensors file. The
[asset manifest](../../src/third_party/components/minilm-assets.json) binds all
sizes and SHA-256 values, including the model card's Apache-2.0 declaration.
Its SHA-256 is `ba5fd8384a05e519fb44b9aa233e94cf82996dbbc15873bdf690cb68465f5c11`.
Two explicit acquisitions used distinct owned directories outside the checkout;
the implemented downloader's real acquisition passed all ten file checks.
Local evidence: `artifacts/embedding-acquisition.log` and the ignored ownership
record `artifacts/upstream/minilm-managed-assets.json`.

An independent PyTorch 2.8.0+cpu / Transformers 4.57.1 reference on CPython
3.13.12 produced all 4 × 384 expected values. The first native comparison had
maximum absolute difference `1.7695128917694092e-7`, below the preselected `1e-5`
tolerance. Evidence: `artifacts/minilm-reference-In3yEA/comparison.json`.
The final [generator and hashed tool requirements](../../src/tests/fixtures/local-embeddings/README.md)
reproduce the same public inputs without consulting the expected vectors.
Three wheels missing hashes in the initial installer report were subsequently
verified against public PyPI digests and reinstalled exactly; those hashes are
included in the complete 25-package reference requirements.

## Executed native implementation

```powershell
pwsh -NoProfile -File scripts/test-embeddings.ps1 -AssetsRoot <explicitly-acquired-model-directory> -OutputRoot artifacts/embedding-qualified-final -TargetRoot artifacts/embedding-target
```

The complete wrapper passed with exit 0:
`artifacts/embedding-qualified-final/4d7e7b6c-5ddb-4aae-b98a-2277589dea25/manifest.json`.
It ran September 18, 2026, 02:25:17–02:26:42 UTC, on Windows 10.0.26200 with
12 logical CPUs, Node 24.10.0, Rust 1.98.0 and MSVC 14.50.35717. VCP base was
`68dbe1fb6c845cbd5ecba117fc50a8b8fb9d2848` with uncommitted implementation.
The manifest retains exact inputs, commands, stage log hashes and binary identity.

| Check | Observed result |
|---|---|
| Asset integrity | All ten files matched fixed sizes/digests |
| Imported source verification | 7,937 Codex and 70 Munarium files matched |
| Library regressions | 3 passed: missing/corrupt setup, input limits, invalid vectors |
| Native dependency graph | 141 packages matched the reviewed reference; required CPU dependencies present |
| Independent reference | All 1,536 values within `1e-5`; maximum difference about `1.77e-7` |
| Batch padding | Single/batch maximum difference about `8.75e-8` |
| Truncation | A long synthetic input reported truncation and 256 active wordpieces |
| Model reopen | Dropping/loading again reproduced all reference vectors within tolerance |

This cached run observed 558 ms loading and 25 ms for the four-text batch.
These single-run timings are feasibility observations, not resource guarantees.
No live provider or paid model calls were made. Python socket interception in
the independent reference is a diagnostic, not a Windows network-denial result.

After binding the fixture to the final generator and hashed reference tools,
the full wrapper passed again:
`artifacts/embedding-qualified-release-inputs/c8569b60-a78d-4cf4-9cc1-f374a9ce89bb/manifest.json`.
The final deterministic command, `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`,
passed all 54 Windows regressions plus repository, source and boundary checks:
`artifacts/tests/bdc1c137-31c4-404f-8e79-321853e87488/manifest.json`.
The updated shared graph also passed all 200 Munarium tests via
`scripts/build.ps1 -Component Munarium -Mode BoundaryTests` and all 102 Codex
tests via `scripts/build.ps1 -Mode BoundaryTests`. Their manifests are
`artifacts/embedding-munarium-regression/6779de53-4988-48b1-b523-2ceddf5b3231/manifest.json`
and `artifacts/embedding-codex-regression/c3db6a44-8d16-4ff5-9fe2-e2584cabcd83/manifest.json`.

## Shared dependency and source changes

The workspace now has 158 local packages in 23 ownership groups with 32 static
source anchors. Of 1,525 preexisting lock entries, only `regex-automata` changes:
0.4.13 to 0.4.14, required by Candle's `fancy-regex` 0.18. Selecting a compatible
tokenizer macro version preserves `pastey` 0.2.1. There are 40 net new entries.
The actual Munarium graph remains 147 packages with the required regex change;
its reference was regenerated from the native graph and provisioned licenses.

| Record | SHA-256 |
|---|---|
| Shared Cargo.lock | `18afa9ec17fe3f3252076f67a4b7b01c207aba4b66b1a68f20db76dc87389328` |
| Third Codex workspace patch | `d866c0cdc1eee368f7684b464e552e6e60822dd2ad9bb7cde07e8abf5a90b80c` |
| Codex resulting file-record array | `a48004db34f6f784c3312995939979ad27b37c81e04ee5e922c5d354d8da7b9f` |

Independent reconstruction matched the modified imported tree exactly:
`artifacts/upstream/codex-embedding-reconstructed.json`. An initial attempt was
rejected by Git's ownership check for the explicitly selected disposable source;
the retry used command-scoped trust for that exact checkout. No global trust
setting or protection was changed. The ordinary build consumes committed bytes
and does not fetch or apply patches.

The Windows CI job acquires pinned model files outside the checkout and runs the
complete qualification alongside the existing Codex, Munarium, Gemini and CLI
trace gates. Local success is separate from the published PR's hosted result;
merge requires both jobs to pass for the exact head.

## Remaining acceptance

The helper neither owns canonical history nor authorizes source reads. P0-02
must integrate scoped text with real Tantivy/DiskANN generations, compare recall
with an exact-vector oracle, reopen a corpus in a fresh process, enforce network
denial at the OS boundary and measure repeated resource use. The controller must
carry asset identity, workspace scope, deletion epochs and pause/reconciliation
into those operations. Missing setup remains visible; a remote embedding fallback
is not permitted. `/pause` while keeping the CLI open remains lifecycle work.
