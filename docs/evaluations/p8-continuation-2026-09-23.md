# P8 continuation — 2026-09-23

Status: production memory integration implemented; targeted observations recorded,
with acceptance gates still open. P8-05 remains
unaccepted; P9 retains its P8-05 prerequisite. This continuation does not record
human judgments, authorize a paid campaign, or authorize publication.

The [September 22 follow-up](p8-qualification-followup-2026-09-22.md) remains the
authority for completed restore, startup, provisioning and retention observations.
Its failed owner attempts and failed interactive observation remain in the record.
New source changes require a new production artifact and affected observations.

## Explicit local-memory integration

P8-01/U09 needs a production adapter for the completed P5-04/05/06 operations.
The selected interface is `memory build --assets DIRECTORY` and
`memory query TEXT --assets DIRECTORY`, with existing query scope filters.
These are explicit CPU operations with resource receipts under an exclusive
canonical owner. They do not need provider credentials or resume a paused task.
`memory search` retains its read-only, non-inference contract. A competing live
owner must be closed before standalone local maintenance can acquire ownership.

Build derives sources from retained authorized evidence and publishes a coherent
generation. Missing assets fail by default; `--allow-lexical-only` explicitly
permits degraded publication. Query failure does not silently become successful
lexical search. Real-assets both-store build/query/reopen, scope and retention
observations passed on the production executable; network-denied qualification
remains a separate observation.

### Native implementation checks

Native Rust 1.95.0 checks passed on the current Windows workstation:

- Three CLI argument/read-only search tests, including explicit asset selection,
  query bounds and no mutation from ordinary search.
- The both-store executable missing-assets/lexical-publication/owner-conflict
  test passed without credentials or a provider profile.
- Nine lifecycle memory tests passed, then all four separately enabled native
  real-assets tests passed, including in-flight cancellation and pause.
- The new path preflight unit test passed.
- Real-assets CLI build/query/reopen/exclusion passed on both stores. An
  independent cosine calculation over each production-code-built two-record,
  two-chunk index matched both ANN ranks (four ranks across two queries/stores).
  This tiny fixture does not establish broad semantic recall or minimum hardware.

Logs are retained under `artifacts/` as `p8-memory-search-tests.log`,
`p8-memory-local-tests.log`, `p8-memory-lifecycle-tests.log`,
`p8-memory-lifecycle-real-tests.log`, `p8-memory-path-tests.log` and
`p8-memory-real-assets-tests-final.log`. These checks used the native debug
executable; they do not substitute for the exact production package observation.
The real-model gate used the fresh pinned assets from the September 22 follow-up.

The first real-assets test failed on an incorrect expectation of immediate
sibling recall after exclusion. Existing deletion-epoch checks invalidate the
entire old generation. The corrected test requires explicit generation-unavailable
and rebuild diagnostics, unchanged old index bytes, then explicit rebuild with
sibling recall and continued target exclusion. The failed log remains
`artifacts/p8-memory-real-assets-tests.log`; no product gate was weakened.

The fast delivery suite passed 16 of 17 groups initially. Source inventory found
the interrupted exploratory build's generated cache inside the vendored tree.
That cache was moved intact to `artifacts/p8-aborted-local-memory-cold-target`;
the single affected group then passed. Receipts:
`artifacts/p8-continuation-fast/aecebafe-5d62-4d6b-921d-b1a8e339540d/manifest.json`
and `artifacts/p8-continuation-fast-recheck/35c6d4a1-f45a-4d91-a7c4-821f5f229e38/manifest.json`.
Imported Codex and Munarium index modes/bytes also passed verification.

After the expanded integration tests and broker corrections, all 17 fast delivery
groups passed in
`artifacts/p8-continuation-final-fast/8de8e981-8a12-454c-bd90-5592e6826a5d/manifest.json`.
Changed Rust files passed formatting checks; the final documentation check covered
403 Markdown files, 2,414 relative links and 56 release work items with no errors.

The v2 owner preparer/runner controls passed 19 tests, and the new offline memory
supervisor passed 26 negative/positive evidence-validator controls. Neither made
provider calls. The supervisor's actual production campaign remains separate.

## Current production observations

The unsigned local candidate executable is
`28c32ddd52c89c4c02b994911162c2b1db229b96f7cf365b20aa6ce1748bf3e3`.
Its source-stable candidate build receipt is
`artifacts/p8-memory-production-final/69cd5a29-e20f-4339-8a17-ff331e1b7101/build-receipt.json`;
source content digest is
`794aaa05dbdddf42e5b02097f3ec4871b583359171ea1e4f18a3cbdd73ccb142`.
Package receipt:
`artifacts/p8-memory-package/0dbe0517-118f-469a-9dda-71b225f15ea5/result.json`;
ZIP digest `4b20079331710e8a7dfac32f1e57e522f76a30c1043cce0d898e147f423938b3`.
No qualification feature, signing, publication or deployment is claimed.

- Real model build/query/reopen, exact-distance comparisons, task filtering,
  generation invalidation after exclusion and explicit rebuild passed on both
  stores (`artifacts/p8-memory-production-real-assets.log`).
- Two populated workspace entries in one registry, distinct canonical roots and
  colliding task IDs passed positive recall and alternating foreign-text queries
  on both stores (`artifacts/p8-memory-production-isolation.log`). Every returned
  scope, source and evidence remained in the selected workspace.
- Missing assets, explicit lexical publication, competing ownership and offline
  optimizer/retention interaction passed against the packaged executable
  (`artifacts/p8-memory-production-controls.log`, two tests covering both stores).
  Optimizer changes invalidate old prune previews; stale interview and prune
  commands leave canonical state unchanged. Empty attempt cohorts remain
  explicitly non-comparable. This is not live routing or policy activation.

All fixture tasks remained paused; no provider attempts, reservations or ledger
entries were created. The optimizer test's initial assertion incorrectly expected
a numeric revision; revisions serialize as decimal strings. The corrected test
uses the existing Revision serializer; product behavior was unchanged. The failed
log remains `artifacts/p8-memory-expanded-tests.log`.

The first network-denied supervisor observation passed independent network
controls but stopped during fixture setup with access denied, before production
memory commands. Its failed receipt is retained at
`artifacts/p8-memory-offline/c31e072e-9d93-42bb-a6f4-5ed3688ef1ab/result.json`.
No offline production pass is inferred from these partial controls.

The next attempt corrected copied executable integrity using the existing
`execution-boundary.ps1` fixture pattern, then stopped on a PowerShell alias
collision before launching production memory. Receipt:
`artifacts/p8-memory-offline/4a63a422-8c51-4d81-8750-dbb026d2db1c/result.json`.
The helper was renamed. The final attempt reached the restricted production CLI
but refused the workspace before memory work:
`artifacts/p8-memory-offline/10cb3ae3-fd1d-496b-b505-420220b4b2f4/result.json`.
Every attempt cleaned its unique profile; all failures remain failures.

A separate native diagnostic opened directories with the production guard's
exact access/share flags. Medium desktop access succeeded; the zero-capability
AppContainer was denied at `C:\` and each external ancestor, while its owned
`AC` and `Temp` succeeded. Evidence is under
`artifacts/p8-memory-offline/path-diagnostic/` (`stdout-3.log`, `report-2.json`).
This is a confinement-harness incompatibility with VCP's ancestor guards, not
evidence that production accessed the network. No external ancestor ACL was
granted and no product path check was weakened. Full production offline
acceptance needs a network-isolated Windows environment with normal filesystem
access. The broker is retained as an honest failed qualification attempt; its
validator self-tests and subsystem canary do not establish that acceptance.
Broker-only corrections after candidate packaging do not change the executable;
their own script digests are in each observation receipt.

## Supplement dispositions

The supplement permits explicit deferred candidate exits. For this first-release
increment, retain the qualified baseline rather than introduce unqualified new
context and verification behavior during final acceptance.

| Increment | Disposition | Evidence and remaining scope |
| --- | --- | --- |
| M5 | Explicitly deferred for first release | Graph/co-change/fusion candidates and their matched comparisons are not implemented or qualified. Existing P2/P5 evidence applies to the baseline only; production memory integration does not qualify these candidates. |
| M6 | Explicitly deferred for first release | Statistical verification ordering has no matched time-to-failure evidence. Preserve the complete required check set and current-revision rules; no performance improvement is claimed. |
| M8 | Explicitly deferred | M4 rejected forecast activation, so no qualified forecast exists for child-budget reuse. Existing atomic root accounting and integrated-parent checks remain required; no forecast savings are claimed. |
| M9 | Partial; acceptance remains open | Seeded numerical fixtures are recorded in [M1 kernel qualification](p6-markov-kernels.md). They do not establish generated lifecycle/store/retention traces or current-package actual delegation. Retain existing adversarial evidence and qualify enabled production boundaries separately. |

These are scope dispositions, not passing comparisons or completion of P8.
The subsequent [bounded seeded campaign](p8-seeded-boundaries-2026-09-23.md)
records additional M9 implementation and execution separately from this snapshot.
M4's rejected/disabled outcome, M7's deferral and M10's post-release placement
remain unchanged. Reconsider M5/M6 only with frozen comparison criteria and
affected scope, retention, resource and current-result qualification.

## Remaining execution gates

The corrected v2 owner launcher needs a fresh versioned preparation bound to the
rebuilt package, current qualified profile and passing launcher controls. The
prior six-slot campaign was one-shot: its unspent dollar allowance does not
authorize replacement attempts. The failed interactive trial retains its $16
unknown-charge reservation; corrected execution needs a separately authorized
bound and fresh artifact/profile binding. Preparation performs no provider calls.

Fresh preparations now exist against the candidate above, with zero calls or
reservations. The six-slot owner plan is under system TEMP at
`vcp-p805-memory-v2-f7d50e63-ea32-423e-a223-2ab4afafe61f/owner-execution-plan.json`,
digest `c65b8d27a532edf18f94e6dc2de5bbe787a03531c4cdab01b78af965fb93b4fa`.
It passed runner validation, binds the corrected v2 launcher and all 23 launcher
controls, and remains explicitly unapproved for spending: six attempts, $8 each,
$48 aggregate, 16 requests/task, no retries. Its launcher grants are limited to
the frozen pagination check; prior failures are not reclassified.
The interactive plan is under system TEMP at
`vcp-p8-interactive-memory-fd2ac140-7b99-49cb-a14c-800734a65597/plan.json`, digest
`10e6507bc1c931415bd630f030011997b62c36125b37f4d0f5a2355c9eed29ba`:
one observation, at most two requests, $16 additional cap, no retries. The prior
unresolved $16 remains held. Explicit authorization for these $64 of new bounds
has been requested; elapsed time is not authorization.

Reuse the six-root owner-history and retention summary
`artifacts/p8-followup/owner-integration-summary-ee688898-0291-4f0c-aab1-46893781a127.json`
(`4dfa2b50078154f8448ea29676b67d62934a989cc67a60278fb9bc65e80ac75a`).
It covers 2,803 event IDs, 101 full-output artifacts and 24 retention controls.
It does not qualify live routing, skill activation, MCP invocation or child work.
Existing qualification-provider executable tests cannot be relabeled as
production-package observations.

Clean Windows, minimum-hardware and physical full-volume observations remain
open. A fresh profile on this workstation is not a clean OS, and constrained
process resources are not independent minimum-hardware evidence. Machine handoff
remains owner-skipped. Human usefulness, correctness, architecture-fit and final
P8-05 acceptance remain owner decisions.
