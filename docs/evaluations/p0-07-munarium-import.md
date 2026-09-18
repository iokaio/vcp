# P0-07 — Munarium import into the shared Cargo workspace

Status: local native checks pass; hosted checks for this import are pending the
PR workflow. P0-07 remains `in_progress`. This does not qualify VCP memory,
local inference, canonical persistence, pause/resume or a release package.

## Source and dependency identity

The [source guide](../development/munarium-source.md) identifies 70 files from
Munarium `8da666067000ca1ee9c131bc67e70b978862faa3`. Only its three library
manifests change: shared-workspace membership and explicit upstream requirements.
Rust implementation and test bodies remain unchanged. Codex's second patch adds
the members and shared lockfile entries, including modification notices.

| Record | SHA-256 |
|---|---|
| Munarium resulting file-record array | `7acb72f0c46a72a099fc42beea65ff0f8dd3e84cc3cca62b29c2c33bf774667c` |
| Munarium manifest patch | `7bb804847d33bc6778550264b39a5d130339836bd3cad342202e4f90685a144c` |
| Codex resulting file-record array | `741eebef081840bdf8061914aacc91385509bf7e3640e3e7ae53e3b3acc3859d` |
| Codex shared-workspace patch | `41e0d1a601e46acdcf61b2228c02250eeebcbff237091a1268a42a40c32c01d1` |
| Shared Cargo.lock bytes | `c815680bd814291776941d30b296b38f079f811f9c981a70f8b623ecb07e5e85` |

Independent reconstruction reproduced both complete inventories. Local records:
`artifacts/upstream/munarium-independent-reconstruction.json` and
`artifacts/upstream/codex-munarium-reconstructed-02.json`. Source checks cover all
70 Munarium and 7,937 Codex files. Cargo metadata and static discovery agree on
157 packages; the catalog has 22 groups and 30 source anchors.

The shared lockfile preserves all 1,491 pre-import Codex package identities and
checksums, adding 34 entries: three selected libraries and 31 external packages.
`cargo +1.98.0 update --workspace --manifest-path src/third_party/codex/codex-rs/Cargo.toml`
produced that resolution. An earlier offline metadata attempt proposed unrelated
changes and failed on an unavailable registry source. Its generated lockfile is
retained in ignored evidence and was not accepted.

Compatible Munarium requirements now resolve some existing Codex versions:
serde 1.0.228 instead of 1.0.229, tokio 1.52.3 instead of 1.53.1 and chrono
0.4.43 instead of 0.4.45. Tantivy remains 0.22.1 and DiskANN 0.56.0. The actual
normal/build graph has 147 packages versus 148 in the standalone baseline.
The [dependency record](../../src/third_party/components/munarium-dependencies.json)
binds versions, registry checksums and declared licenses to the shared lockfile;
it is not a release notice bundle.

Embedded-data review also matched the 127-line English stopword fixture exactly
to PostgreSQL 16.15 commit `7d3e000c5961a544302072058a1184e9a588837b`.
The inherited Rust array remains unchanged. PostgreSQL/Snowball notices and the
fixture's byte digest are retained in [third-party attribution](../../THIRD_PARTY_NOTICES.md#munarium-local-library-source).

## Executed native checks

```powershell
pwsh -NoProfile -File scripts/build.ps1 -Component Munarium -Mode BoundaryTests -OutputRoot artifacts/upstream/munarium-shared-final -TargetRoot artifacts/upstream/munarium-target
pwsh -NoProfile -File scripts/build.ps1 -OutputRoot artifacts/upstream/codex-shared-build -TargetRoot artifacts/upstream/codex-target
```

Munarium passed 200 tests: 68 core, 92 datastore unit, 8 contract vectors,
4 DiskANN, 4 lexical, 13 roundtrip and 11 reference-store tests. One upstream
performance benchmark was ignored; no performance envelope is claimed.
The dependency check passed, requiring Tantivy/DiskANN and rejecting the
identified server/provider/PostgreSQL packages. Names do not prove absence of
network effects.

Final library manifest:
`artifacts/upstream/munarium-shared-final/1d2931ce-b3b4-4e68-98ff-9ac50dd38db6/manifest.json`.
It records VCP base `d6b3ec0371161e52630ed4d8c5f266646ac00d9c` with a dirty
implementation tree, source hashes, exact command and exit 0. The cached run
took about 21 seconds on September 18, 2026 UTC: Windows 10.0.26200, 12 logical
CPUs, 68,622,794,752 bytes RAM, Rust/Cargo 1.98.0, MSVC 14.50.35717,
CMake 4.2.3-msvc3, Ninja 1.12.1 and Node 24.10.0. Test log SHA-256:
`f4b5b8c3add5c44bada92d15936b2c932bfccd5582a9a05d9fa4b2c7584ccf21`;
normalized dependency record:
`224bbf9636ca56a1cc5f96ac3788977c4dcbaac845f7080b1836a9bb5faaf554`.

Codex's native CLI build passed with its ordinary Rust 1.95.0 compiler:
`artifacts/upstream/codex-shared-build/e77eb6f0-725c-4b5a-9361-82058b3014d3/manifest.json`,
log SHA-256
`97c571b2290c338f5d6a63cf1e3dc9fa8d173950b45a438c9c6c3c2b575857f0`.
That build preceded the lockfile modification-comment addition; its manifest
preserves the earlier inventory identity. The resolved graph and Rust code were
unchanged. CI rebuilds the final committed inputs from a clean runner.

The final Codex source inventory also passed all 102 patch/policy tests via
`scripts/build.ps1 -Mode BoundaryTests`, recorded at
`artifacts/upstream/codex-shared-tests/48349ed8-5fa0-4956-b624-4643afcf1526/manifest.json`.
After adding reference-drift enforcement and output containment for both source
trees, the final wrapper reran all 200 Munarium tests successfully:
`artifacts/upstream/munarium-shared-reference/b5199a15-b40c-450f-bbbd-e1a302b1e92f/manifest.json`.
The dependency-license generator reproduced the committed reference exactly.
Its [documented command](../development/munarium-source.md#dependency-and-effect-boundaries)
uses the actual native dependency record and provisioned package declarations.

## Delivery and remaining gates

The boundary resolver allows only explicitly selected external component roots.
A regression tests default rejection, authorized sibling membership, outside
path dependencies and junction/symlink escapes. Imported whitespace and Markdown
links retain upstream bytes; VCP-owned documentation remains checked.
Additional regressions reject writes into the other component's source tree
before allocating output and reject changed dependency versions, membership or
lockfile identity. The fast suite covers source inventories, documentation links,
68-task ownership, 56-task release closure and deterministic test contracts.

The workflow targets `ubuntu-8core` for deterministic checks and `win8core` for
native qualification. Windows installs both compilers, builds Codex, runs its
patch/policy tests and scripted CLI traces, tests Munarium and reconstructs both
selections. Local success does not establish remote CI success for this change.

One Cargo graph is established, but component compiler pins remain separate.
No embedding runtime/model is installed or invoked. Gemini selection and
remaining effect classification still gate P0-07. P0-02 owns CPU inference,
reopen/recall, resource measurements and offline enforcement. P0-03/P0-08 own
lifecycle and authority adapters, including `/pause` while the CLI remains open.
Tests use public/synthetic fixtures and make no paid model calls.
