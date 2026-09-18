# Verified local CPU embeddings

P0-07 adds `src/crates/vcp-embedding/` to the existing Codex Cargo workspace.
It provides file-only MiniLM loading and bounded CPU embeddings for qualification.
The [runtime/asset record](../../src/third_party/components/embedding-runtime.md)
and [evaluation](../evaluations/p0-07-local-embeddings.md) identify exact inputs
and executed evidence. P0-02 adds [observed offline qualification](offline-embeddings.md) and real
corpus/index integration plus [declared resource measurements](local-memory-resources.md).
[ADR-008](../adr/008-local-governed-memory.md) records the bounded selection and
remaining product integration gates.

## Explicit asset acquisition

Install Node 24 and the [native development tools](codex-source.md#setup-and-ordinary-build),
including Rust 1.98.0. Then choose a new model directory outside the source
checkout. The command below makes a per-run directory beneath the OS temporary
root; a persistent installation can instead use an explicitly chosen local model
cache. Model assets, runtime databases and indexes do not belong in the checkout.

```powershell
$modelRoot = Join-Path ([IO.Path]::GetTempPath()) ('vcp-minilm-' + [guid]::NewGuid())
node scripts/upstream/model-assets.cjs acquire --root $modelRoot
node scripts/upstream/model-assets.cjs verify --root $modelRoot
pwsh -NoProfile -File scripts/test-embeddings.ps1 -AssetsRoot $modelRoot
```

Stop if acquisition or verification fails. Acquisition uses the fixed Hugging
Face repository and immutable revision from the compiled asset specification,
without provider credentials or automatic authentication. It streams each file
with a size bound, checks SHA-256, flushes verified bytes and publishes the file
only after verification. Existing destinations are rejected. Failed downloads
remain partial and the ownership record stays `failed`; retry with a fresh
directory instead of treating partial files as a valid installation.

Ten selected files total 91,578,299 bytes. The main safetensors file is
90,868,376 bytes. The model card's Apache-2.0 declaration is recorded separately
from the software package licenses. No model download occurs during `MiniLm::load`
or `embed`; missing assets produce `MissingAsset`, corrupt/incompatible bytes
produce `InvalidAsset`, and the qualification command returns `not_run`/3 for a
missing model. There is no remote embedding fallback.

## Library behavior and boundaries

`MiniLm::load(&Path)` verifies every selected asset. It retains verified config,
tokenizer and weight buffers and consumes those same bytes, avoiding a separate
unchecked reopen or mutable memory map. The loader selects the fixed six-layer
BERT architecture and CPU device. It does not use a Hub client or an arbitrary
model loader that can import remote code.

`embed(&[&str])` accepts 1–16 nonempty texts, each at most 65,536 UTF-8 bytes.
The pinned tokenizer truncates to 256 wordpieces including special tokens.
Results expose active token counts and an explicit truncation flag. Attention
and pooling exclude batch padding. Masked mean vectors are L2-normalized to 384
dimensions; zero, nonfinite and wrong-shaped results fail before returning an
embedding. Missing setup, invalid input and runtime errors remain distinct.

This helper does not authorize workspace reads or publish an index. The future
controller must supply permitted text and the authorized model root, bind the
asset specification digest to chunk/index generations, and apply pause,
workspace-scope and deletion rules. Model computation cannot promote recalled
content into authority. Follow [the memory design](../architecture/memory-retrieval-design.md)
and [engine design](../architecture/engine-execution-design.md).

## Native qualification and dependency provenance

`scripts/test-embeddings.ps1` verifies assets and both imported source inventories,
runs the three library regression tests, builds the real qualification binary,
checks its actual native normal/build dependency graph and runs the four model
checks. `-OutputRoot`, `-TargetRoot` and `-Jobs` select explicit evidence/cache
paths and 1–16 build jobs. Outputs reject source/asset containment, including
resolved aliases. Missing native tools or assets are `not_run`, and nonzero
stage exits fail the command. CI has an overall Windows job time limit; the
wrapper is not a new process sandbox or durable task supervisor.

Each UUID manifest records source commit/dirty state, input/fixture hashes,
compiler/native tools, lockfile and asset specification, commands, stage logs,
binary hash, numerical differences and observed timings. These measurements
describe the executed host/run, not a supported resource envelope. An interrupted
record is not a pass. Raw evidence belongs in ignored `artifacts/`; no model
weights, generated profiles or environment dumps belong in published reports.

The shared graph has 158 local packages. Its third Codex patch adds this helper
and 40 net lock entries. The only changed preexisting package is `regex-automata`
0.4.13 to 0.4.14, required by Candle's `fancy-regex` 0.18. The compatible tokenizer
macro version preserves `pastey` 0.2.1. Both native Codex and Munarium gates must
pass against this graph. [Patch provenance](../../src/third_party/patches/codex/README.md)
and independent source reconstruction remain separate from ordinary compilation.

The embedding graph has 141 packages. The checker requires Candle, Tokenizers
and safetensors, rejects identified HTTP/Hub/GPU packages, and compares exact
versions plus the lockfile digest with the reviewed reference. This bounds the
selected dependency graph; it cannot prove absence of all network effects.
After explicit dependency provisioning, reproduce a new license-declaration record:

```powershell
cargo +1.98.0 tree --manifest-path src/third_party/codex/codex-rs/Cargo.toml -p vcp-embedding --locked --offline --target x86_64-pc-windows-msvc --edges normal,build --prefix none --format '{p}' > artifacts/embedding-dependencies.log
node scripts/upstream/record-embedding-dependencies.cjs artifacts/embedding-dependencies.log artifacts/embedding-dependencies-reviewed.json
```

Compare the result with `src/third_party/components/embedding-dependencies.json`.
The generator reads provisioned public package declarations, never downloads,
and rejects missing/ambiguous identities or absent license declarations. Update
the affected Munarium reference after any shared lockfile/graph change as well.
Neither record is a complete release notice bundle.

## Independent reference and CI

[Reference fixtures](../../src/tests/fixtures/local-embeddings/README.md) retain
four original synthetic texts, independent PyTorch vectors, the generator and
hashed maintenance tool requirements. The native executable compares all vector
values and unit norms, a padded batch against a single input, visible truncation,
and loading the model again after dropping it. No paid model calls are used.

The manual Windows qualification job acquires the exact model under `RUNNER_TEMP`, runs this
wrapper on the standard `windows-2025` runner, and uploads only qualification evidence.
The Codex, Munarium, Gemini and CLI trace gates remain required in that manual run.
Deterministic asset/dependency regressions also run on `ubuntu-24.04` without
downloading the model. Subsequent P0-02 gates cover
[corpus reopen and exact-vector recall](local-memory-spike.md),
[OS-enforced network denial and missing/corrupt assets](offline-embeddings.md),
and [declared resource measurements](local-memory-resources.md).
Product setup and `/pause` integration remain future work; these qualification
functions alone do not implement them.
