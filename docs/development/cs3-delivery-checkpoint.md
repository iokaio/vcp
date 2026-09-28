# CS-3 delivery checkpoint — 2026-09-28

CS-3 remains open. This increment implements and tests independent qualification
work; it does not waive the original-artifact regrade or claim six successful
skill comparisons. No candidate is promoted into the default catalog.

## Acceptance evidence

- [Adversarial boundary qualification](cs3-boundary-acceptance.md): actual Windows
  positive/negative filesystem, junction, network and job-breakaway controls.
- [Frozen WEB execution](cs3-frozen-web-execution.md): real browser interactions,
  six frozen case outcomes and source-bound lifecycle controls.
- [Generated UI execution](cs3-generated-ui-execution.md): exact returned HTML,
  isolated-world observations, genuine input and independent receipt validation.
  All 23 final-source native controls passed qualification: six positive grades
  and 17 deliberate negative grades matched their exact expected vectors, with
  independently verified process drainage and profile removal. This does not
  substitute for regrading the missing original CS-2 outputs.
- [Package acceptance](cs3-package-acceptance.md): distinct baseline/candidate
  install, upgrade, rollback and re-upgrade; exact-installed offline discovery,
  explicit activation and persisted revocation for all six candidates.
- [Six-skill readiness](cs3-six-skill-readiness.md): prospective corrections,
  bounded comparison implementation, pinned DeepSeek/OpenRouter selection and
  the approved USD 100 envelope. No new paid comparison calls were made.

## Regression checks

The initial full fast-suite attempt is retained at
`artifacts/cs3-delivery-check-native/a8d19453-0494-4a27-8a4e-a26115bc1ccc`.
It was not green. Targeted reruns preserve the failed attempts rather than
rewriting them or claiming a later complete-suite run.

The first committed draft, `dba1dfdc`, subsequently passed the complete Linux
"Repository and harness" CI job, including the SDK/editor tests, registered fast
suite and imported-source verification:
[workflow run 36468972991](https://github.com/iokaio/vcp/actions/runs/36468972991).
The two opt-in Windows workflow jobs were skipped by their existing event
conditions; the local native receipts remain separate evidence, not claims that
those skipped jobs ran. Later source changes require their own CI result.
The downloaded fast-suite manifest retains 23/23 registered cases passing under
the PR's synthetic merge commit `e43bbafccc6d91abafc8395c362ebc999dedc828`:
`artifacts/cs3-ci-36468972991/f4992ba7-ee00-47dc-9009-872f8372ce7f/manifest.json`,
SHA-256 `012f4e11ade352a3032a2e447edc9cd7aea9dc4a1bd612115af63e3f5a4e3f7b`.

The receipt-backpressure and collector corrections also passed the complete
Repository and harness job, most recently at source commit `12c502fd` in
[workflow run 36471857260](https://github.com/iokaio/vcp/actions/runs/36471857260).
Its retained fast-suite manifest records 23/23 passing cases under synthetic merge
commit `a8a5025c88ecd9e2aa05c7b8d77dfb9d1a0b98e2`:
`artifacts/cs3-ci-36471857260/98d408f1-8980-4a21-b3dd-38cb6ccbebb2/manifest.json`,
SHA-256 `451afe4e95a286354fa90cf25b0e053c057555c3a1dce739eb0bb86963f62386`.
The opt-in Windows jobs were again skipped; local native qualification is separate.

The teardown-order correction `45b7ed6d` passed the same complete job in
[workflow run 36473617548](https://github.com/iokaio/vcp/actions/runs/36473617548),
with 23/23 fast-suite cases under synthetic merge commit
`970f931a0e7d2c2ce936abaf158721cf138571c8`:
`artifacts/cs3-ci-36473617548/41e1a1b2-6d3b-4cc3-8b84-7459e7cb6293/manifest.json`,
SHA-256 `1801cce59b967fef09fbfd00c0c1dae27a98eb19b76751dbff728292c686af32`.

| Check | Retained result |
|---|---|
| CS-2 developer | 83/83 passed; `artifacts/cs3-reviewed-developer-check/4fb3b649-ca5b-4693-844c-c0a897849f8c/manifest.json` |
| Builtin contracts | 47 passed, five existing environment-dependent skips; `artifacts/cs3-reviewed-builtin-check/af057019-2445-440a-b137-9dae1c52a56c/manifest.json` |
| CS-3 focused contracts | 71/71 passed after final native qualification and public-validator integration; `artifacts/cs3-final-qualified-contract-check/d3174124-06d1-463f-a70f-6eaa007aa02b/manifest.json` |
| CS-3 comparison host | 9/9 passed, including frozen external-builtin drift; `artifacts/tests/02ad9e15-cd48-45e9-9cf7-39a96c29b20b/manifest.json` |
| CS-1 authoring | 132/132 passed, zero skips; `artifacts/cs3-post-build-authoring-1800/55fa722c-d142-45f8-a040-fd6e06b87388/manifest.json` |
| Upstream source | Passed after relocating the generated default Cargo target; `artifacts/cs3-clean-upstream-check/adc82dde-4a37-40e3-9325-6f223c5abc3f/manifest.json` |
| Live runner / bootstrap / P8 qualification | Passed with the physical Node executable; receipts `0d4b40aa-3e0f-4958-b0d4-eb74d03c5b82`, `a884a8b8-fa29-4eeb-97ef-75e3668e6381`, `08982d7a-5973-4bf1-8d27-f12a3db91f95` under `artifacts/cs3-physical-node-check` |

The final repository gate passed with 656 Markdown files, 2,940 relative links,
68 tasks and 56 release tasks:
`artifacts/cs3-final-qualified-repository-check/2fdf363c-50ad-428e-9785-23bc00901569/manifest.json`.
All ten changed PowerShell scripts parsed without errors; whitespace checks and
a changed-line credential-pattern scan passed. The public UI validator is the
same implementation used for native projection and rejects fabricated native
evidence even when its source closure is correctly bound.

The developer expiry fixture now advances a test-only clock explicitly. It
retains refusal before any claim and refusal after expiry, without depending on
how quickly Windows hashes the source inventory. The production clock is unchanged.
Measured developer execution took about 811 seconds; the registry allows 1,200.
Builtin execution previously took about 89 seconds and the final registered run
took about 43; the registry allows 180. Two 600-second authoring attempts timed
out with active tests, so its harness allowance is 1,800 seconds; the completed
132-test attempt took about 1,408 seconds. These are
test-harness scheduling limits, not relaxed product or browser deadlines.

The native receipt writer retries only Windows access/sharing errors for the
same owned atomic replacement, bounded to ten attempts. Before each attempt it
rechecks the exact path and old/new hashes; unrelated errors or changed bytes
fail immediately. A real Windows lock test verifies both released-lock success
and persistent-lock failure with the original exception and unchanged files:
`artifacts/cs3-receipt-replacement-native-e98e0452eebb402b99c641ba59d8c347/result.json`,
SHA-256 `5d3af8401c0bc8b2b38c0a20fe7b4a4b83d3d14de89b28f1d5abf32dfbb497d3`.
Buffered flush on close precedes replacement; power-loss durability is not claimed.

A later idle native control exposed receipt-write backpressure: UI assertions
and 9/9 process coverage completed, but the unchanged 15-second drain ACK deadline
expired. A read-only replay of its 157 events measured 157 full serializations,
82,673,965 cumulative bytes and 4,647 milliseconds before filesystem overhead.
The controller now checkpoints ownership immediately after registration and before
resume, retains all prelaunch/pause/cleanup intent writes, and saves the complete
bounded event stream during cleanup. It sends ACK immediately after independent
job-zero observation, without an intervening disk write. The same replay needs
ten ownership serializations, 5,030,722 bytes and 342 milliseconds. Both independent
reviews and 46 pure protocol/policy checks pass; final native evidence must bind
this newer source. Startup and ACK deadlines were not increased. An abrupt crash
can lose ordinary events since the last checkpoint; such partial receipts cannot
qualify normal execution and remain eligible only for exact owner-loss recovery.

A later pseudo-motion control returned its exact expected UI failure vector,
9/9 process coverage and drain acknowledgement, but the handle collector retained
a strict job-PID inventory exception. That run remains failed; the original
assigned/returned counts were not logged, so a teardown race is not a proven
diagnosis. The supervisor now stops and joins the collector and validates its
held identities before deliberate job termination. Each step has an independent
cleanup boundary, so a stop or collection failure cannot skip termination.
Teardown polls the job's active count, matching the existing worker guardian;
live PID inventory checks, exact lifetime-process coverage and the independent
job-zero acknowledgement remain mandatory. Sixteen injected-failure assertions
cover every combination of stop, collection and termination failure. New bounded
PID-count diagnostics preserve the counts if a future strict inventory check
fails. Final qualification binds this corrected source: the focused pseudo-motion
control and all 22 remaining controls passed without retries on that source.
WEB26 then passed all six frozen oracles plus pause, cancellation and controller
owner loss on the same source closure. Its six-case aggregate is
`artifacts/cs3-frozen-web-26/web-oracles.json`, SHA-256
`a5bb97a72a07650d2682808de02d5f9fd71d445758167685652ab2ad6956dab5`.
The linked execution records retain exact native identities and prior failures.

## External prerequisite

The [retained UI recovery record](cs3-retained-ui-recovery.md) identifies the
missing authenticated CS-2 packet index, six packets and private label mapping.
Local artifacts, Git objects, named backups and eligible older VCP sessions did
not recover those bytes. Hashes, source-review summaries and newly generated
outputs cannot stand in for them. The importer authenticates recovered originals
before staging, but a synthetic import is not a historical browser regrade.

The comparison preparation gate remains closed while that prerequisite is absent.
The current CS-3 exit condition is therefore unsatisfied, regardless of the
independent engineering and test results above. Restoring the authentic retained
bundle is required to continue that acceptance path; changing the requirement
would be an explicit owner decision, not an implementation assumption.
