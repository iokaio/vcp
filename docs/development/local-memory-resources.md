# Local memory resource qualification

P0-02 extends the [governed corpus prototype](local-governance-spike.md) with
declared 100, 1,000 and 10,000-record datasets, native Windows memory counters,
sampled disk usage and repeated queries. The gate uses real CPU MiniLM vectors,
Tantivy and DiskANN. It preserves the independent relevance and exact-vector
checks; increasing the corpus does not lower their required recall.

The [memory design](../architecture/memory-retrieval-design.md),
[ADR-008](../adr/008-local-governed-memory.md) and
[P0-02](../plan/01-upstream-feasibility.md#p0-02--local-munarium-and-search-spike)
own this feasibility work. These measurements qualify an integration candidate,
not an installed VCP application, durable canonical recovery or minimum hardware.
See [reviewed results](../evaluations/p0-02-local-resources.md).

## Commands and source map

Use the [native tools and verified assets](local-embeddings.md), then run:

```powershell
pwsh -NoProfile -File scripts/test-local-memory.ps1 -AssetsRoot $modelRoot -Scale
```

`-TargetRoot` can reuse the existing embedding target directory. The wrapper
first verifies imported source and assets, runs the native regression tests,
builds the executable, checks the unchanged 214-package dependency closure and
runs the original 24-document gate including receipt/manifest corruption. The
optional scale gate then executes all three sizes. CI supplies `-Scale` on the
configured Windows runner; no private credentials or paid calls are needed.

For an already qualified executable:

```powershell
node scripts/upstream/trace-memory-resources.cjs --binary artifacts/embedding-target/x86_64-pc-windows-msvc/release/vcp-memory-spike.exe --assets $modelRoot --output-root artifacts/local-memory/resources
```

| Source | Responsibility |
|---|---|
| `src/tests/fixtures/local-memory/scales.json` | Frozen generator version, seed, three sizes, five warm repetitions, exact recall requirement and observation deadlines |
| `src/tests/support/memory-resources.cjs` | Original deterministic corpus expansion, independent result checks and bounded-root disk inspection |
| `scripts/upstream/trace-memory-resources.cjs` | Owned index/scratch roots, native process orchestration, sampled logical disk usage and cross-process comparison |
| `src/crates/vcp-memory-spike/src/main.rs` | Explicit bounded corpus input, identity-bound receipts, batch timings, index construction and repeated query/oracle execution |
| `src/crates/vcp-memory-spike/src/resources.rs` | Native process memory observation with an owned sampling thread |
| `src/tests/contracts/memory-resources.test.cjs` | Frozen dataset identities and false-positive rejection regressions |

No third-party source, Cargo dependencies or reconstruction patches change.
The memory sampler is an explicit effect in the qualification boundary catalog.
It is not an injected product controller, scheduler or resource-enforcement policy.

## Dataset and independent acceptance

The generator retains all 24 original records, seven original relevance cases
and their supersession history. Original synthetic specimen records extend both
workspaces equally. Their shared marker identifiers have different descriptions
in each scope. Two new lexical queries require the correct workspace's marker.
The seed versions public fixture content; it is never used for keys or nonces.
Generated UTF-8 bytes and SHA-256 are retained with every attempt. Regression
tests freeze each of the three dataset identities.

An explicit `--corpus <fixture>` suffix enables the measured native experiment.
The parser reads at most 64 MiB, accepts at most 10,000 records and 64 queries,
rejects unknown fields/duplicate identities, and confines fixture workspace
names to `atlas`/`boreal`. Truth references must identify current records in the
query's workspace. The actual gate/projection determines visibility; the fixture
flags are its independent oracle. Receipts bind the exact input bytes and model
specification before opening an index. Ordinary invocations retain the compiled
24-document fixture.

For each size, the observer starts three fresh native processes: build, reopen
and second reopen. Each query process records nine cases once, then five more
times per case with the same loaded model and open index. Every one of the 54
rows must preserve scope, current-version visibility, required relevance and
recall@3 of 1 against the independent f64 exhaustive cosine oracle. Results must
remain equal across repetitions and both fresh query processes. All three sizes
therefore require 324 checked query rows, not a single repeated success summary.

The existing whole-corpus candidate request/filter remains visible in these
measurements. Its cost is not presented as production ANN latency. The retained
governance adapter also replays every public claim into a volatile backend on
each fresh process. Durable projections, incremental admission and scoped
candidate refill remain work for their owning product adapters.

## Timing boundaries

Separate timings cover governance replay, model loading, each embedding batch,
index construction, validated reopen, query inference, index lookup/filtering
and oracle work. Batch records include actual item counts, including partial
batches, and must account for every document. The first build batch is the first
inference on that newly loaded model; later batches have different input shapes,
so their times are observations rather than a controlled first/warm comparison.

Query validation re-embeds the whole corpus to supply the independent oracle.
`oracle_inference_ms` records that additional work separately from user-query
inference; `oracle_compare_us` records exhaustive scoring separately from lookup.
The first recorded query for a case already follows oracle inference. It is not
a cold model request. Five subsequent samples use the same query/model/index.
The process work time includes validation, while the outer attempt also includes
startup and fixture parsing. Do not add repeated per-query reopen values; the
workspace timing records identify each actual open operation.

Fresh processes start without an in-process model/index cache. OS file caches
are not purged, and no cold-disk claim is made. Retain individual samples and
host/source identity; these small samples do not establish production percentiles
or a latency SLA. Each phase has a declared 900-second completion limit.

## Memory, disk and cleanup

The Rust sampler inspects only its own process. It reads Windows x64 native
counters and walks committed virtual-address regions every requested 50 ms,
also taking an initial and final observation. It records the actual largest
sample gap. The sampling thread is joined on completion and dropped on errors;
missing API data or a failed sampler cannot pass qualification.

| Measurement | Meaning |
|---|---|
| Resident and peak resident bytes | Current working set and the OS-maintained peak read through the last sample |
| Private committed and peak private committed bytes | Private commit charge and its OS-maintained peak, not resident RAM |
| Sampled committed mapped-address bytes | `MEM_MAPPED` committed virtual regions, including file/pagefile-backed views |
| Sampled committed image-address bytes | `MEM_IMAGE` committed virtual regions, reported separately |

These overlapping measures must not be added together as a RAM requirement.
Mapped virtual bytes are not necessarily resident or uniquely owned physical
memory. The native regression creates and touches an owned 16 MiB mapping,
observes its appearance/removal and the retained peak, then frees its handles.

The parent samples only its newly allocated index and scratch roots every
requested 100 ms. It records maximum observed logical file lengths, separate
scratch usage, retained index length, sample count and actual maximum gap.
Temporary files can disappear during observation; other I/O errors and links
fail the experiment. The measurements exclude model assets and source files,
whose sizes are recorded separately. Logical lengths exclude filesystem metadata,
allocation rounding and compression effects; sampled peaks can miss short spikes.

Every native child receives a dedicated `TEMP`/`TMP` directory through the
existing harness's environment allowlist. Successful attempts require an empty
scratch root after process exit. Indexes, generated corpora, logs and failed
attempts remain in the owned ignored evidence directory for inspection. Do not
clean unrelated directories or reset a failed index to make a check pass.
The harness bounds each process tree and caps output at 2 MiB per phase.

Manifests retain the binary, source/helper/model/corpus hashes, CPU model/count,
RAM, OS/architecture/runtime, compiler and filesystem information, phase outcomes,
all query samples and measurement limits. The [offline embedding gate](offline-embeddings.md)
separately qualifies the network boundary. Neither gate is a final installed
package or a product pause/recovery implementation.

## Primary measurement references

- [PROCESS_MEMORY_COUNTERS_EX](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex)
  and [GetProcessMemoryInfo](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-getprocessmemoryinfo)
  define working-set and private commit counters.
- [VirtualQuery](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualquery)
  and [MEMORY_BASIC_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-memory_basic_information)
  define the region walk and explain why mapped classifications are not a
  measurement of private physical residency.
- [SYSTEM_INFO](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/ns-sysinfoapi-system_info)
  supplies the current process's application-address bounds.
