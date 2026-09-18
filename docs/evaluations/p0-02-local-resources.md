# P0-02 — Declared local memory resource experiments

Task state: `complete` for P0-02's bounded feasibility acceptance, combining the
source, corpus, governance, offline and resource evidence linked below. The new
resource gate passed locally; its PR must pass remote CI before merging.
The [resource gate](../development/local-memory-resources.md) expands the real
governed CPU/Tantivy/DiskANN experiment to 100, 1,000 and 10,000 records. These
are synthetic feasibility measurements, not an installed application or a
production capacity/latency promise.

## Accepted base

The base is `e93ec3f7f28e42c458b6b489d85a9cfdce696cd4`, the merge of
[PR #20](https://github.com/iokaio/vcp/pull/20). Its exact head
`256fec1cca344cb6792bded2295d8b4252f675c5` passed
[run 35314128300](https://github.com/iokaio/vcp/actions/runs/35314128300).
Windows used `win8core-1000002563` in `wingroup`; repository checks used
`ubuntu-8core-1000002564` in `ubuntu8core`.

Downloaded offline evidence is under `artifacts/ci/windows-35314128300-1/`.
The Windows embedding wrapper manifest is
`embeddings/e33b00a8-d849-4f4f-91b6-3d7d3547bf18/manifest.json`; its nested
offline manifest is `offline/394fe55b-411b-4e11-a917-a6d6469afec5/manifest.json`.
All five profile cleanups completed, both controls connected, contained inference
passed, and missing/corrupt asset copies returned their expected failures.
The artifact contains no `private/` configuration/journal directory. These are
the preceding offline gate's results, not CI results for the new scaling gate.

## Current native qualification

Command:

```powershell
pwsh -NoProfile -File scripts/test-local-memory.ps1 -AssetsRoot $modelRoot -Scale
```

The native host is Windows 10.0.26200 x64, Intel Core i7-10750H at its reported
2.60 GHz base frequency, twelve logical CPUs and 68,622,794,752 reported RAM
bytes. Node is 24.10.0; compilation uses Rust 1.98.0
(`88d9e12ae`, 2026-08-18), MSVC 14.50.35717, NTFS and four build jobs.
No OS file cache flush, remote model call or paid service
is part of this experiment.

| Evidence identity | Value |
|---|---|
| Wrapper manifest | `artifacts/local-memory/f2876e92-ce9e-46c3-99ef-fc42c55b7862/manifest.json` |
| Nested scale manifest | `resources/b5732978-6f1a-4513-9bbd-5528e18bdd87/manifest.json` |
| Native binary SHA-256 | `e05187f77fb964a394b0caa7611ba95649edf1c3d518dc8aa19cc9d7157638c4` |
| Qualification executable bytes | 11,346,944 |
| Model asset bytes | 91,578,299 across ten verified files |
| Model specification SHA-256 | `ba5fd8384a05e519fb44b9aa233e94cf82996dbbc15873bdf690cb68465f5c11` |

The experiment has nine native resource phases: build and two fresh query
processes per size. Each query process must verify nine cases across the first
recorded pass and five warm repetitions. Every one of 324 query rows must retain
scope/current-version relevance and exact-oracle recall@3 of 1. Missing phases,
samples, batches, memory fields or completed scratch cleanup cannot pass.

The initial native regression run passed twelve tests, including the owned
Windows mapping/counter test and explicit corpus identity/input rejection.
Six new deterministic observer tests pass, freezing corpus bytes and rejecting
misleading resource, repetition, scope, oracle and disk evidence. The shared
workspace stays at 159 packages and the memory closure at 214. The effect catalog
now covers 47 entries in 23 groups: one pure, three read-only and 43 effectful.
No third-party source, dependency versions or reconstruction patches change.

The wrapper passed all nine stages with exit 0, from 06:55:14 through 07:20:07 UTC
on September 18, 2026. The nested resource experiment ran from 07:00:01 through
07:20:07 UTC. All nine native phases validated, all 324 query rows passed and all
nine scratch roots were empty after exit. The original corpus gate also passed
its receipt and manifest corruption checks.

| Records | Phase | Process work ms | Governance ms | Model load ms | Peak resident bytes | Peak private commit bytes |
|---:|---|---:|---:|---:|---:|---:|
| 100 | Build | 2,146 | 21 | 509 | 187,691,008 | 227,643,392 |
| 100 | Reopen | 2,360 | 27 | 501 | 187,813,888 | 227,672,064 |
| 100 | Second reopen | 2,329 | 21 | 465 | 187,793,408 | 227,713,024 |
| 1,000 | Build | 14,765 | 1,869 | 473 | 190,611,456 | 230,383,616 |
| 1,000 | Reopen | 15,453 | 2,039 | 463 | 190,459,904 | 230,666,240 |
| 1,000 | Second reopen | 14,218 | 1,746 | 478 | 190,345,216 | 230,895,616 |
| 10,000 | Build | 380,349 | 258,845 | 439 | 202,756,096 | 248,602,624 |
| 10,000 | Reopen | 389,716 | 254,567 | 446 | 202,940,416 | 248,467,456 |
| 10,000 | Second reopen | 382,680 | 252,344 | 506 | 202,354,688 | 245,809,152 |

The following ranges include both fresh query processes, 90 warm samples per
size. They are observed minima/maxima, not population percentiles. Reopen is the
range over four actual workspace opens; model inference and oracle work are
excluded from the lookup/filter range.

| Records | Build embedding ms, both scopes | Build index ms, both scopes | Workspace reopen ms | Warm query inference us | Warm lookup/filter us | Retained index bytes | Largest sampled index + scratch bytes |
|---:|---:|---:|---|---|---|---:|---:|
| 100 | 1,294 | 304 | 8–12 | 5,495–12,538 | 90–465 | 272,031 | 281,665 |
| 1,000 | 11,918 | 489 | 24–27 | 5,594–12,186 | 1,122–2,423 | 2,542,881 | 2,579,371 |
| 10,000 | 119,103 | 1,939 | 147–331 | 5,882–84,330 | 68,373–162,067 | 25,310,262 | 25,569,123 |

First build batches contained 16 items and took 229,659 / 216,555 / 234,296 us
at the three sizes. Aggregate embedding throughput was approximately 77.3 / 83.9 /
84.0 records/s; batch shapes differ. Independent oracle re-embedding took
1,302–1,312 / 11,401–12,310 / 124,566–127,027 ms per query process. Individual
batch, query and exact-oracle comparison measurements remain in the manifest.
Maximum sampled mapped-address bytes were 1,724,416 / 1,740,800 / 2,007,040;
largest sampled scratch lengths were 10,290 / 45,415 / 313,838 bytes. Across all
phases the maximum observed sampling gaps were 103 ms for memory and 131.4 ms
for disk, against requested intervals of 50 and 100 ms respectively.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed eight cases and
73 tests, exit 0, with manifest
`artifacts/tests/1f3023fe-0664-4b7c-9085-7cdf32db8fc5/manifest.json`.
The missing-assets preflight was corrected after the resource controller had
started: absent assets now produce `not_run`/exit 3 before any native phase.
Its focused regression and this final fast suite passed; the successful native
execution path and binary were unchanged. The run manifest retains the original
controller hash rather than claiming it ran the later preflight edit. CI will
exercise the final committed controller and full scale gate.

## Dataset and interpretation

The generator preserves the original 24 records and seven relevance cases. Both
workspaces receive equally many original synthetic specimen records. Shared
marker identifiers have different scoped descriptions; two extra lexical cases
assert the correct workspace result. The superseded original record remains
stored/indexed but excluded from current views. Dataset identities are:

| Records | UTF-8 bytes | SHA-256 |
|---:|---:|---|
| 100 | 30,199 | `b7607f1198a98c8898912545aef31fe31885d8c4f938c1ced5fe4ea55667a9f0` |
| 1,000 | 289,735 | `87edb8318f5ea2a68330fd8812d6554eb996726c39ce0738bcd4301fdcb187ef` |
| 10,000 | 2,895,149 | `410eded2229bb3f16ee64038ff6e705abdd045524f540a0e38eaae81e1af01ac` |

Each dataset has one superseded record; current visibility therefore contains
one fewer record than the build, and both versions remain available to the
governance history checks. These controlled distractors and nine cases do not
establish general semantic quality or workload diversity.

Governance replay, model load, per-batch inference, index construction, validated
reopen, query inference and lookup/filtering are recorded separately. Re-embedding
all corpus records for the independent oracle is extra qualification work;
`oracle_inference_ms` and `oracle_compare_us` expose it. The first recorded query
already follows oracle inference, so it is not called a cold model request.
Fresh process/model loading is distinguished from OS cold-disk behavior.

Resident/private commit peaks come from native OS counters observed through the
final sample. Committed mapped/image address ranges and logical file lengths
are sampled and can miss brief peaks. Virtual mapped bytes are not physical
residency, private commit is not resident RAM, and these measures must not be
added together. The helper/model/index sizes are experimental components, not
the footprint of an installed VCP release.

## Preliminary probes and remaining integration

Ignored preliminary experiments under `artifacts/network-proposal/` established
API layouts against a touched 16 MiB native mapping and ran the same retained
governance adapter at three scales. A 10,000-record governance-only replay took
239,515 ms on this host. The preliminary full corpus run
`scale-579aef90-7dd8-4910-88a9-277a58ca54cc/manifest.json` passed build and 54 query
checks but is not substituted for the final Cargo-built gate above.

These probes exposed material prototype costs: every fresh process rebuilds the
volatile governance view, and candidate filtering requests the whole workspace's
candidate set. The final measurements keep those costs visible. P0-04/P1/P5 must
introduce durable, revision-bound projections and scoped candidate refill without
weakening admission, current authority, provenance or history retention. A cache
or a precomputed fixture flag cannot replace canonical authorization.

Product deletion/revision races, crash durability, pause/recovery, installed
packaging, broader real-world retrieval quality and final performance thresholds
remain owned by their subsequent plans. Full-index AppContainer compatibility is
also separate from the [accepted offline CPU experiment](p0-02-offline-embeddings.md).
See the [implementation and measurement references](../development/local-memory-resources.md).
