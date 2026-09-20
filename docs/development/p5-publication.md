# P5-05 coherent search publication and recovery

P5-05 builds private lexical/vector components against one authorized canonical
inventory, then activates their manifest, workspace pointer and covered indexing
intents in one revision-checked canonical transaction. Component timestamps do
not establish compatibility. Missing vectors leave a visible coverage deficit
and do not acknowledge semantic sequence progress.
An explicit `empty_complete` manifest represents a fully empty eligible inventory
without inventing a vector file. It can acknowledge fully represented excluded
intents; reopening proves the retained inventory is actually empty.

The manifest records canonical watermark, memory sequence, current authority and
deletion revisions, inventory identity, lexical/tokenizer specification, complete
embedding specification, component checksums, covered intents and predecessor.
Raw retained records remain authoritative. The optional recent-change overlay
is lexical-only and bounded to 128 records, 256 KiB and 50 ms; overflow reports
unsatisfied freshness rather than advancing its through-watermark.

Private writes and actual component reopen precede canonical activation.
Windows validation capabilities retain read handles denying writes and deletion,
including generation directories, through activation. Opened-handle reparse
checks reject namespace substitution. Capabilities are bound to their publisher;
all publishers for the same canonicalized private root share reader pins. The
publisher holds its root directory against replacement and preparation pins the
new generation and lexical directories while writing them.
Non-Windows activation explicitly requires separate qualification.

Recovery starts at the canonical active pointer and visits at most 64 retained
predecessors. It reports failed derived components and either selects a valid
compatible reader or requires rebuilding. Canonical corruption remains an error.
Exact publication retries consult the canonical receipt even if the derivative
has subsequently been damaged. Cleanup requires explicit historical/retention
policy and refuses active generations, live readers, validation capabilities and
snapshot pins. Cleanup never removes canonical claim or source evidence.
Ordinary store snapshots hold a shared `root-snapshot.lock` lease, including
snapshots without artifacts. Cleanup holds the exclusive lease through deletion;
snapshots surviving owner close/reopen still prevent it. The lock is never
unlinked as part of cleanup.

The canonical host uses its existing process-wide local admission pool. Vector
inference, lexical preparation and component validation run away from the owner
thread; the final activation performs canonical checks and a transaction. The
source fence permits the operation's own resource receipt while requiring source
identity, content, applicability, authority and deletion state to remain stable.
An explicit lexical-only option makes missing-vector degradation observable;
inspection does not start maintenance.
Both preparation stages retain separate local CPU/RAM/disk observations. The
lexical stage samples new private generation bytes at component barriers and
checks its declared reservation. Those checks and bounded native writer inputs
are not OS allocation quotas; native calls remain nonpreemptible. A 60-second
deadline prevents subsequent work. Resource errors retain their diagnostic cause.

## Qualification

The native run killed actual subprocesses at eight boundaries for each
backend: before/after lexical, vector, manifest and active-pointer publication.
All sixteen reopened the previous complete generation or the new committed one,
without acknowledging an incomplete intent. Nine publication tests, three
canonical coverage tests and two snapshot-lock tests passed, including namespace
swap denial, competing publishers, lost-reply recovery after derivative damage,
bounded overlays, empty/excluded coverage and stale authority/deletion epochs.
Three host tests passed on both backends: real CPU vector adoption from retained
native source artifacts, explicit missing-asset degradation and complete-empty
publication without opening model assets.

Logs are `artifacts/p5-05-final-publication.log` and `p5-05-host-verified.log`.
The affected domain/protocol/store/memory regression run passed 103 tests with
two opt-in native tests excluded from that broad run. The publication crash test
was run explicitly above; actual embedding execution is also covered by the
source-backed host gate and P5-04 qualification. Clippy, the production CLI build
and all eight fast delivery checks passed. No new warning was reported in the
changed code; existing datastore, store-backend and lifecycle warnings remain.

Reproduce using the native Windows build environment and externally provisioned
`VCP_MINILM_ASSETS`:

```text
cargo +stable test --locked --offline -j4 -p vcp-memory --test publication -p vcp-store --test search_contract --test snapshot_pin -- --include-ignored
cargo +stable test --locked --offline -j4 -p vcp-lifecycle --test canonical_host memory_publication:: -- --include-ignored --test-threads=1 --nocapture
cargo +stable test --locked --offline -j4 -p vcp-domain -p vcp-protocol -p vcp-store -p vcp-memory --tests
cargo +stable clippy --locked --offline -j4 -p vcp-store -p vcp-memory -p vcp-lifecycle --all-targets --no-deps
cargo +stable build --locked --offline -j4 -p vcp-cli --bin vcp
scripts/test.ps1 -Suite fast
```

Other logs: `p5-05-regression.log`, `p5-05-clippy.log`, `p5-05-build.log` and
`p5-05-fast-final.log`, under `artifacts/`. Local qualification artifacts are not
committed; the committed tests reproduce the boundaries.
