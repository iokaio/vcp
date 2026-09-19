# P2 deterministic context-compaction increment

Status: pure module and native context contracts passed on September 19, 2026.
P2-08 remains `in_progress`; full continuity acceptance is not complete.
The [guide](../development/p2-context-continuity.md)
defines the qualified boundaries.

The module previews older completed tool pairs with explicit omissions and
original source references, preserving recent pairs in full. It rejects current
task fields, unfinished pairs, cross-scope inputs and malformed correlations.
Revalidation compares current revisions, recomputes the saved projection and
reads original sources through current history access. Only its process-local
validation result can create a captured untrusted-history part.

Four contracts exercise UTF-8 previews, unchanged originals, complete retained
pairs, current-field rejection, changed steering/deletion/scope, altered or
unreadable sources, modified saved summaries and no-gain behavior. The native
context suite also retains existing repository, instruction, source-pinning and
selection regressions. No model requests or new dependencies are involved.

The first pure build exposed an ambiguous fixture error-type import, corrected
with an explicit context error import. The four focused tests passed afterward.
An initial 27-test context run passed; final API review then made the validated
result a prerequisite for constructing the summary part. The final suite below
passed with that stronger API and stable input hashes.

## Final native command

`pwsh -NoProfile -File scripts/test-context.ps1 -Jobs 2 -TargetRoot
C:/code/Github/vcp/artifacts/context-continuity-target -OutputRoot
C:/code/Github/vcp/artifacts/context-continuity` ran from an isolated checkout.
Output paths are disposable local qualification resources; the script's ordinary
defaults work from a clean checkout with the documented prerequisites.

Result: pass, all 27 tests, exit 0. Manifest:
`artifacts/context-continuity/df94a1f9-9db4-40fa-8aeb-d700634173de/manifest.json`,
SHA-256 `9d374c724463597a250ae1895d3bab0ae2ba21c78972f47c6205e2e9f3574cf1`.
Native environment: Windows `10.0.26200`, NTFS, MSVC `14.50.35717`, experimental
Rust `1.98.0 (88d9e12ae 2026-08-18)`. The original baseline pin is unchanged.

Fast delivery/source checks passed (exit 0), using `scripts/test.ps1 -Suite fast`
with an isolated output root. Manifest:
`artifacts/context-continuity-fast/d06b7b0a-c2e1-4069-9e3b-826410515db2/manifest.json`,
SHA-256 `f40971c9e2ce62523c3303204725aaa2a2a14a6e2b900308815b6b726e69b97a`.
Staged source bytes are checked against the final native manifest before commit.

## Retained owner integration

The owner now explicitly enables deterministic compaction after setting its
verification baseline. Actual request assembly pins the latest objective,
constraints, instructions, original/current source manifests, effect outcomes,
verification records and quantitative accounting outside historical summaries.
Pending charges retain their exact reservation, currency, liability and uncertainty.
Complete canonical records and all original provider/tool artifacts remain intact.

The adapter revalidates original history access and current sources, measures
gain on the actual encoded provider request, captures immutable projection
provenance and fences admission on current effect/accounting observations.
The pure module's gain remains content bytes; the host's gain is serialized
request bytes, using the qualified conservative estimate rather than claimed
provider-token counts. No extra model call is made for compaction.

The retained fixture drives five real read calls, then closes/reopens the owner
twice on each store. Between owners it changes source and scoped instructions,
accepts a late correction, and observes a response without cost. The next
deliberately resumed request must preserve the exact unresolved charge, original
baseline and current correction while retaining one complete recent pair.
All five original pair artifacts are compared byte for byte after reopen.
A second trace enlarges historical previews and requires capacity failure,
paused state, retained liability and zero additional HTTP requests.
These are fresh owner lifetimes in one test process, not independent-process
crash or console-close evidence.

Development attempts exposed a fixture type-inference error and oversized
mandatory requests. Shorter previews alone still exceeded the usable 22,464-byte
input budget (24,000 less output and safety reserves). The final representation
removes repeated completed-effect metadata and uses explicit pending-charge
fields plus complete-record digests. It preserves full unresolved/stopped effects
and does not raise model capacity or omit uncertainty. An added fixture assertion
was corrected to inspect the typed amount's currency and micros fields.

`pwsh -NoProfile -File scripts/test-integration.ps1 -Jobs 2` passed all 44
native host/port/canonical contracts, both retained process-launch regressions
and the seven-request private CLI trace (exit 0). Manifest:
`artifacts/integration/a582a31f-07f4-47b0-8c34-723ce8079671/manifest.json`,
SHA-256 `13d9964e6eea57056d024a598b45a296f42e404ffbdb4318fa928c8befea944d`.
The focused continuity regression separately passed its four store/scenario
traces in 125.50 seconds. The integration run then passed it with the final
formatted source and stable input hashes.

`pwsh -NoProfile -File scripts/test-p1.ps1 -Jobs 2` passed all 85 foundation
contracts (exit 0). Manifest:
`artifacts/p1/0504555c-cd4a-4666-8464-2b0c2c969edd/manifest.json`,
SHA-256 `90496476fdc5a7fec9270079437109f5cb54ffa2b264d2d4f85b07aa9c62c6be`.

The retained baseline runner passed both `RecoveryTests` and `LifecycleTests`
with `-SelectedCodex -ExperimentToolchain 1.98.0 -Jobs 2`, using
`artifacts/build` and `artifacts/upstream/codex-target` for evidence and builds.
Both commands exited 0. Their manifests are:

- Recovery: `artifacts/build/5f6f9943-e458-4fd1-98a4-03a82575996a/manifest.json`,
  SHA-256 `a33c6a4787b31341063cb3dce6d5235ed53236ab3b610d09178c167b2440d716`.
- Lifecycle: `artifacts/build/88817a53-7c68-4d28-bc97-2dc23bbea887/manifest.json`,
  SHA-256 `c7d7966a2fc271653f9cb16e23be8413183695c4afd699e56a2903a9f2739b6c`.

These existing recovery regressions supplement the continuity-specific traces;
they do not establish independent-process recovery of a compacted coding turn.

Final `scripts/test.ps1 -Suite fast` delivery/source checks passed (exit 0).
Manifest: `artifacts/tests/1383cdf5-92de-4860-9056-9ac8de41030b/manifest.json`,
SHA-256 `c2a841fc8863dbdc2623c6e3d9f9ab3499d82c7447d71a79f7a9651f53bb92f9`.
The preceding sandboxed attempt could not hash Git source and exited 1 before
running checks; the approved native run passed without changing the checks.

## Remaining acceptance

Full current text diffs and Git/index state, broader decision/acceptance
projections, pending-pair recovery and incompatible-provider handoff still need
qualification. Source manifests alone do not establish full diff continuity.
No semantic summarizer, hidden helper request, new model asset or installed CLI
is claimed by this increment.
