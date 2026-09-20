# P5-04 local vectors and bounded CPU maintenance

The actual CPU embedding/DiskANN network-denial campaign passed on Windows
10.0.26200 x64 with Rust 1.98.1, 2026-09-19. Canonical generation activation is
separate P5-05 work.

`src/crates/vcp-memory/examples/vector_quality.rs` and
`scripts/upstream/trace-offline-vectors.cjs` exercise the production adapters.
The Rust example reuses
`vcp-embedding/src/bin/qualify/network.rs` by source path, so it keeps the existing
Windows firewall diagnostic and private-address canary contract unchanged.

The new Node driver reuses these existing implementations unchanged:

- `src/tests/support/windows/offline-embedding.ps1`
- `src/tests/support/windows/AppContainerFixture.cs`
- `src/tests/support/offline-embeddings.cjs` broker, blocked-canary and traffic checks
- The normal qualification `runSuite` process/capture supervision and model asset verifier.

Use the already provisioned external MiniLM directory; do not download assets
inside the experiment. Assets are verified against the existing pinned model
manifest, copied into a disposable profile and verified again by the broker and
actual model loader. Build with the existing native MSVC environment:

```powershell
$env:CARGO_TARGET_DIR = 'D:/code/Github/vcp/artifacts/codex-target'
# Run from src/third_party/codex/codex-rs.
cargo +stable build --locked --offline -p vcp-memory --example vector_quality -j4
# Run from the repository root; replace the external-assets path.
node scripts/upstream/trace-offline-vectors.cjs --binary artifacts/codex-target/debug/examples/vector_quality.exe --assets <existing-external-assets> --output-root artifacts/p5-vector-offline
```

The stages are unrestricted control-before, AppContainer inference, AppContainer
missing-config, AppContainer corrupt-config and unrestricted control-after.
The driver requires zero capabilities, token SID/profile agreement, complete
capture, expected exit codes, positive measured job memory, completed cleanup,
and exactly the two expected nonces at an independently observed live listener.
A blocked connection or a child's self-report alone cannot pass. Native Windows
and a private IPv4 address are required; absence reports `not_run`.

The existing 30-second worker deadline remains unchanged. Exceeding it is failure,
not a silently relaxed bound. The missing/corrupt stages intentionally expect
the new LocalEmbedding adapter's load-stage failure and exit 1; the adapter maps
the lower-level typed asset failure to its own error. This does not change the
existing P0 validator or its expected missing-asset exit code.

Inside the restricted process, the actual `LocalEmbedding` loads pinned CPU
MiniLM and embeds current public fixture sources. `Component` builds real DiskANN,
saves a private component, reopens it and checks exact retained vector identities.
All seven fixture queries compare ANN top-3 chunks against separately computed
exhaustive cosine top-3 with a declared small-fixture minimum recall of 1.0.
Three repeated query observations retain IDs, distances' ordering, measured query
time and explicitly reported ANN mode. Narrow source authorization, empty
authorization and foreign workspaces are checked separately. Cache equivalence,
incompatible dimensions and component corruption are also tested.

The fixture has two workspaces and a superseded source. Chunk oracle recall is
distinct from semantic source-label diagnostics; this report does not measure
lexical fusion. Its caller-supplied source scope does not claim canonical access
integration or P5-05 activation qualification. Measured job memory is an observed
peak for this bounded fixture, not a production capacity guarantee.

The parent records source/binary/model hashes, Windows/Node versions, stage
reports, token observations and observed traffic in the normal manifest.
No credentials or recovery material are needed.

## Observed qualification

The five-stage campaign succeeded: both positive controls connected, while
inference and both asset-failure cases ran with zero-capability AppContainer
tokens and denied network access. The independent listener observed exactly
the two permitted control nonces. Missing/corrupt stages retain their expected
failure exit status; they are not presented as successful inference. Every
temporary profile reported completed cleanup.

Evidence is `artifacts/p5-vector-offline/73aca68b-2e2c-415d-bd6b-67f28941309b/manifest.json`.
The inference process took 9.434 seconds and peaked at 228,442,112 job-committed
bytes. Model load took 1.952 seconds. Atlas/Boreal each produced 12 vector chunks
from 11/12 current sources; embedding took 1.673/1.062 seconds, graph building
1.824/1.651 ms, and validated reopen 18.280/18.493 ms. All seven queries across
three repetitions matched the exhaustive chunk oracle at recall@3 1.0. Source
semantic labels are separate diagnostics; no fused retrieval result is claimed.

The embedding specification digest is
`6ad732172605147b2e0bbb1a40d48d8edd08ff8c1416232f1be7c73746015a56`;
the pinned asset specification is
`ba5fd8384a05e519fb44b9aa233e94cf82996dbbc15873bdf690cb68465f5c11`.
The same AMD Threadripper PRO 5975WX host described in
[lexical qualification](p5-lexical.md) ran this debug build. These small-fixture
measurements occurred while native build work ran elsewhere on the host; they
are observed timings, not an isolated performance or capacity guarantee.

## Production boundaries

The complete specification binds assets, CPU runtime, tokenizer, preprocessing,
192-byte UTF-8 subchunks, rejected truncation, 384 dimensions, mean pooling,
L2 normalization and cosine distance. Cache identities include workspace/source,
original byte span, content digest and the complete specification. Batches are
at most 16 and a component has at most 1,024 chunks. Missing assets never select
a remote endpoint. DiskANN node ordinals remain internal; persisted native graph
headers, counts, IDs, floats, centroid and edges are bounded and validated against
the retained canonical row table before the provider can allocate from headers.

ANN overfetch is bounded at 256; an authorized subset of at most 128 can use an
explicit exact fallback. Larger unsatisfied selective requests report reduced
recall. No branch removes the caller's source predicate. Component writes check
serialized size before creation and reject immediate-parent/file reparse points.

The host's explicit `build_memory_vectors` API captures current authorized
inventory, admits one process-wide local build, and moves CPU work off the
canonical owner thread. The RAII admission stays held through model teardown,
including when the caller drops its future. Owner pause/close, authority/deletion
changes, external cancellation, queued interactive work and a 60-second deadline
stop subsequent bounded units. Native model batches and graph construction are
not preemptible. A changed canonical watermark prevents handing back a ready
component. Inspection never starts this work.

Declared admission estimates reserve model/activation margin plus bounded
vectors/source buffers against 1 GiB RAM and 64 MiB temporary-disk ceilings;
they are workload reservations, not OS allocation quotas. Temporary writes have
prewrite checks. Resource receipts separately record process CPU deltas, sampled
resident/private/mapped-address and temporary-disk peaks, maximum sample gaps,
and OS process-lifetime high-water marks. Mapped address extent is not resident
mapped-page usage. Current-host resource observations incur no paid model charge.

The final Files/SQLite successful host runs took 1.987/1.961 seconds, with
3,750/3,734 ms process CPU deltas. Fifteen samples per run observed resident
peaks of 183,435,264/192,040,960 bytes, private committed peaks of
152,883,200/157,663,232 bytes, mapped address peaks of 4,706,304/4,739,072 bytes
and temporary files of 17,015/17,016 bytes. Maximum sample gaps were
1,896/1,869 ms; these sparse samples do not establish instantaneous build peaks.
Separate OS lifetime working-set peaks were 242,163,712/273,539,072 bytes.

## Verification

All 62 memory tests and four native canonical-host vector tests passed, including
the normally opt-in real-model tests. The host tests cover pre-admission pause,
missing assets, retained resource receipts, real build/reopen, and in-flight
cancellation/root pause on both stores. The latter require responsive owner
operations, no ready component after interruption and released admission for
subsequent work. Adapter tests include corrupt-but-rehashed graph mappings,
bounded selective fallback, incompatible specifications, private-path reparse
rejection and prewrite disk refusal.

```text
cargo +stable test --locked --offline -j4 -p vcp-memory --tests -- --include-ignored
cargo +stable test --locked --offline -j4 -p vcp-lifecycle --test canonical_host memory_vectors:: -- --include-ignored --test-threads=1 --nocapture
```

These require `VCP_MINILM_ASSETS` to identify the provisioned external pinned
asset directory and the repository's native Windows build environment. Logs
are `artifacts/p5-04-tests-final.log` and `artifacts/p5-04-host-final.log`.
The eight fast delivery checks, imported source inventory, boundary checks
and exact patch replay passed. No model assets are committed.
The shared process-admission unit test, production CLI build and Clippy also
passed. Clippy reports no warning in the changed memory crate; existing
lifecycle warnings and the pinned datastore's disabled-feature unreachable-code
warning remain. Logs are `p5-04-admission-final.log`, `p5-04-build-final.log`,
`p5-04-clippy.log` and `p5-04-clippy-clean.log` under `artifacts/`.
