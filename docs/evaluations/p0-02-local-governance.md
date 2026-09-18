# P0-02 — Scoped local governance qualification

Task state: `in_progress`. The original
[governance adapter](../development/local-governance-spike.md) connects the real
corpus experiment to the retained Munarium kernel and volatile backend. This
report does not qualify durable canonical recovery, production access control,
offline OS enforcement or the complete U09 acceptance case.

## Inputs and change

The base is merge `1033bb6122d690b58df2713370d3f5de3c67c49e` from
[PR #18](https://github.com/iokaio/vcp/pull/18). Its exact head
`7d197ecf54af55e53f9896728fee19d17978abb4` passed
[CI run 35307412196](https://github.com/iokaio/vcp/actions/runs/35307412196):
Windows used `win8core-1000002555` in `wingroup`, and repository checks used
`ubuntu-8core-1000002556` in `ubuntu8core`. Downloaded Windows corpus evidence
`60ea7501-cc7b-46e2-ba4b-56f9b383e0ac` records eight passing stages; observer
`abcd0bb0-91ea-4523-b59e-b31646f5ff44` records seven queries with recall@3 of 1.
These are the preceding corpus baseline, not CI results for the new adapter.

The new adapter uses Munarium core/store APIs at
`8da666067000ca1ee9c131bc67e70b978862faa3`. Corpus version 2 adds an explicit
pause-claim predecessor; all 24 texts and seven relevance expectations remain
unchanged. The original `current` flags serve only as the independent oracle.
Each process records synthetic proposals in a separate backend per workspace,
resolves accepted visibility through `slice_facts`, and filters real Tantivy and
DiskANN candidates before the result limit.

The native normal/build closure contains 214 previously reviewed packages,
including the original executable. The five newly reachable packages relative
to the preceding corpus closure are `munarium-core` 1.2.1,
`munarium-store-mem` 1.2.1, `chrono` 0.4.43, `mio` 1.2.0 and
`tokio-macros` 2.7.0. No package versions or checksums change. The shared lock
retains all 1,566 entries; only the local package gains three dependencies.

| Input | SHA-256 |
|---|---|
| Shared lockfile | `0311a5464f4611d0b0f4cace839674560f5d113789fb205b43a90d7d20ad02fd` |
| Fifth Codex patch | `177155c25a263206529b0bc48943ab7267ad6d9a9f18b993e0a8a3614e090e69` |
| Reconstructed 7,937-file Codex selection | `29fc055c76bc52cc4461f31a23540667f3f2cd74e994bb56efe45054166f3b8b` |
| Corpus version 2 | `f5bc7532611892da77cc4e9ab793054d01ac2475e316ba530895c9d0ea08337e` |
| Governance adapter source | `23a6a0939b23ad615538791f0e69a32c8e9ebdc14486924474c210ee62eb2302` |
| Native executable | `9a92c1ca32915cd337d24dd07abe5c217b6d153ab7f2fb1689eac922208d99fc` |

## Validation

The eight native Rust tests pass in the shared Cargo workspace. Five new
governance cases cover retained conflicts, rejected corrections under a locked
anchor, scoped/stale authority, matching current predecessors and unique IDs,
and historical corpus replay. Existing exact-vector, receipt and pre-limit
filtering checks remain in place.

The complete `fast` suite passes all eight cases and 63 regressions. Its manifest
is `artifacts/tests/27ff1704-8c06-4191-91d4-9f8111eed969/manifest.json`.
Ten focused observer/dependency tests also pass. Independent source reconstruction
matches every committed Codex path; the static catalog still covers 159
packages, 23 groups and 45 source entries.

The managed native command also passed, exit 0, on September 18, 2026,
05:10:30–05:14:33 UTC:

```powershell
pwsh -NoProfile -File scripts/test-local-memory.ps1 -AssetsRoot <verified-external-model-directory> -TargetRoot artifacts/embedding-target
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

The environment was Windows x64 `10.0.26200`, Node 24.10.0,
Rust 1.98.0 (`88d9e12ae`), MSVC 14.50.35717, four build jobs and 12 logical
processors. The native manifest records the base commit plus a dirty implementation
tree and hashes its actual code/fixture inputs. Raw evidence stays under ignored
`artifacts/local-memory/0d362bc6-a492-4683-85d0-aede53184d08/manifest.json`;
its observer is `trace/dcfdf6e9-3ad4-4fd4-a052-66e9dd44d28e/manifest.json`.

All eight managed stages passed, including eight Rust tests and the checked
214-package native graph. Build and both fresh-process queries reported the same
governed visibility:

| Workspace | Recorded proposals | Current records | Historical replacement checks |
|---|---:|---:|---:|
| Atlas | 12 | 11 | 1 |
| Boreal | 12 | 12 | 0 |

All seven queries retained their required relevance and scope; raw ANN recall@3
remained 1 in both processes. The corpus has no rejected proposals: rejection
retention is exercised separately by the native gate tests described above.
Wrong receipt and altered manifest phases still returned the intended exit 1;
their raw failed attempts are explicitly marked expected rejections. No assertion
or relevance expectation was weakened. Hosted CI for this implementation remains
pending until the exact-head PR checks complete.

## Interpretation and limits

The retained resolver considers supersession edges before its status filter.
Accordingly, the adapter records a rejected correction as a disputed proposal
without an effective replacement edge. The original proposed predecessor and
gate findings remain evidence. Tests independently retrieve that proposal,
the accepted predecessor and its finding; accepted corrections still supersede
only in views at or after their sequence.

The backend is deliberately volatile. Reopening the real indexes in a fresh
process replays the compiled synthetic claims; it does not recover durable
governance state. Claim and findings writes are separate, so this does not
qualify transactional proposal acceptance. P0-04/P5 own durable integration,
live access revisions, deletion and recovery. P0-02 still needs broader corpus
and resource measurements, model-failure integration and observed OS network
denial. The pause descriptions in the fixture do not implement `/pause`.
