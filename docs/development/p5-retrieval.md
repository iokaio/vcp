# P5-06 hybrid retrieval and inspection

Retrieval captures an authorized canonical inventory and pins its store snapshot,
searches compatible components outside the owner thread, then reconstructs every
returned passage from current canonical bytes. Persisted index text is never
returned as evidence. Current authority, deletion epoch, source identities and
workspace binding are checked again before return.

Fusion is `rrf-equal-k60-id-ascending/1`: equal reciprocal rank contributions,
constant 60, and stable record ID ties. Native component scores and ranks remain
visible. Repeated vector subchunks contribute once per source record; overlapping
source spans and repeated claim versions are deduplicated. Exact authorized IDs
filter Tantivy before its top-candidate limit, so stronger out-of-scope matches
cannot crowd out an eligible result.

Passages retain claim/source version identities, byte spans, evidence references,
and accepted/inferred/disputed labels. Trimming preserves those fields and UTF-8
boundaries. The serialized passage-array byte count is a conservative token upper
bound, rather than an estimate from an unrelated provider tokenizer. Responses
include generation and canonical watermarks, indexed memory sequence, fixed
degradation codes and an explicit rebuild requirement. Denied content does not
appear in exclusion diagnostics. Historical requests without a compatible
generation require rebuilding; current vectors are never labeled historical.

The host admits explicit local query embedding through the shared local resource
pool. Model loading and inference retain CPU, memory and disk observations,
including post-load and post-inference samples. The opaque selected context is
bound to its owner, scope and exact labeled context part. Provider preparation
and dispatch recheck the canonical fence and actual native source files. The
last check occurs after reservation and capture, immediately before submission;
a failure releases a definitely unsent reservation. Retry performs the same
checks. Ordinary context preparation cannot reuse an unfenced memory artifact.

Inspection is read-only and remains available while paused. It uses retained
lexical components without starting embedding, extraction or index repair.
Both the live owner and offline CLI use the same bounded canonical source
discovery and retrieval service. Missing components produce an explicit degraded
response. One request deadline covers discovery, recovery, search and return;
dropped asynchronous inspection cancels subsequent detached work. Native calls
are checked at their boundaries and are not preemptible OS resource quotas.

```text
vcp memory search "retry accounting" --task <task-id> --limit 8
vcp memory search "submit" --root <root-id> --path src/worker.rs --symbol submit
vcp memory search "retention" --minimum-sequence 42 --tokens 4096
```

Paths are exact relative source paths. Task, root, path and symbol constraints
intersect. The CLI allows up to 64 results and 16,384 tokens, with a five-second
deadline and 65,536-byte response-passage bound. `--historical` requests a declared
historical memory sequence; unavailable history returns a rebuild requirement.
Structured results expose provenance and component ranks. Inspection does not
silently compute a query vector, so semantic-vector degradation is explicit.

## Verification

The affected domain, protocol, store, context and memory regression run passed
123 tests. Two opt-in native tests were excluded from that broad run; native
inference and publication crash coverage have separate explicit runs. The CLI
contract and search targets passed nine tests. Four host query tests passed,
including actual MiniLM inference on both stores, eight source/dispatch fence
cases, paused inspection and missing assets. Inspector cancellation and checked
publication recovery each passed their focused test.
The complete canonical-host regression also passed 33 tests in 714 seconds,
with four opt-in model tests excluded. The new query model gate was run explicitly
above; publication/vector gates retain their P5-04/05 qualification evidence.

The final query inference runs retained four resource samples each, including
samples while model allocations were resident. Files/SQLite process CPU deltas
were 4,797/5,266 ms and elapsed inference work was 2,038/2,046 ms. Sparse process
samples do not establish instantaneous allocation peaks or isolated model cost.

```text
cargo +stable test --locked --offline -j4 -p vcp-domain -p vcp-protocol -p vcp-store -p vcp-context -p vcp-memory --tests
cargo +stable test --locked --offline -j4 -p vcp-cli --test contracts --test memory_search
cargo +stable test --locked --offline -j4 -p vcp-lifecycle --test canonical_host memory_query:: -- --include-ignored --test-threads=1
cargo +stable test --locked --offline -j4 -p vcp-lifecycle --test canonical_host -- --test-threads=1
cargo +stable test --locked --offline -j4 -p vcp-lifecycle --lib memory_inspection::
cargo +stable test --locked --offline -j4 -p vcp-memory --test publication recovery_cancellation
```

Native inference requires the pinned external `VCP_MINILM_ASSETS` and the
established Windows build environment. Logs are under `artifacts/p5-06-*.log`.
All eight fast delivery checks and formatting of changed Rust files passed.
Package-wide formatting also detects existing differences in unrelated lifecycle
examples, journal and tests; these files were preserved.

The production CLI build and Clippy passed. No warning was reported in the
changed retrieval code; existing imported and lifecycle warnings remain.

## Native hybrid campaign

The unchanged P5-03/04 public fixture contains 24 source versions, 23 current
sources, two workspaces and seven labeled symbol/intent queries. Its SHA-256 is
`f5bc7532611892da77cc4e9ab793054d01ac2475e316ba530895c9d0ea08337e`.
The pinned MiniLM embedding specification is
`6ad732172605147b2e0bbb1a40d48d8edd08ff8c1416232f1be7c73746015a56`.
Labels were not retuned. The runner independently checks canonical source IDs,
scope, exact exhaustive vector neighbors and lexical/vector/fused relevance.

On Windows 10.0.26200, Threadripper 5975WX (64 logical processors), 137,295,024,128
bytes RAM, debug build, both Files and SQLite passed 126 query repetitions,
42 scope checks and four canonical generation builds. Each of the seven queries
had lexical, vector and fused union-label recall@3 of 1.0; ANN versus exhaustive
recall@3 was also 1.0. This small synthetic set establishes fixture behavior,
not production-scale quality or an advantage over lexical retrieval.

| Backend/workspace | Warm hybrid p50/p95 ms | Reader reopen p50/p95 ms |
|---|---:|---:|
| Files/atlas | 48.147 / 50.679 | 23.004 / 23.324 |
| Files/boreal | 51.225 / 53.151 | 23.243 / 23.929 |
| SQLite/atlas | 47.647 / 52.042 | 22.822 / 23.190 |
| SQLite/boreal | 50.829 / 53.221 | 22.993 / 23.303 |

Warm query timing excludes the separately measured local query embedding.
Reader reopen is not an end-to-end cold query; filesystem caches were not
flushed. Four canonical component build/publication times were 196–203 ms,
excluding inference. Model loading took 1.936 seconds. The embedding job's OS
peak committed memory was 229,076,992 bytes, which is not resident memory or a
per-model allocation measurement. Host receipts above provide separate sampled
CPU/RAM observations.

All actual source and query embedding ran in a zero-capability AppContainer.
The independent broker observed the matching token SID, successful canaries
before/after the restricted campaign, and zero restricted connections. Missing
and corrupt assets failed at model loading. Canonical store publication and
retrieval ran in a trusted process consuming a bounded, hash-checked vector
bundle with exact specification, source digest, scope and span validation.
That canonical process was not network blocked and performed no model inference.
Four altered bundles (source digest, scope, query digest and vector) were rejected
before any generation build.

The original single-process attempt failed because Windows DOS-volume path
resolution used by `std::fs::canonicalize` is denied in this AppContainer.
Diagnostic probes confirmed locking, flush and hard-link publication worked.
Production path guards and network controls were preserved. This failed attempt
is retained under `artifacts/p5-hybrid-offline/b1353e11-1795-4a20-8af6-22ce65b926b7`;
the first successful composite run overlapped a build and is retained separately.
The final quiet run is
`artifacts/p5-hybrid-offline/1b44cdca-dea5-429f-836e-73285d903402/manifest.json`,
with `summary.json` and `handoff-rejections.json` beside it. Manifests retain
source/binary/asset hashes, queries, exact oracles, process observations, failures
and cleanup evidence.

```text
cargo +stable build --locked --offline -j4 -p vcp-memory --example hybrid_quality
node scripts/upstream/trace-offline-hybrid.cjs --binary <hybrid_quality.exe> --assets <pinned-external-assets> --output-root artifacts/p5-hybrid-offline
```
