# P2 deterministic context-compaction increment

Status: pure module and native context contracts passed on September 19, 2026.
P2-08 remains `in_progress`; retained-host integration and full continuity
acceptance are not complete. The [guide](../development/p2-context-continuity.md)
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

## Remaining acceptance

The retained owner must integrate compaction with current objective/constraints,
diff/base, effects, verification and accounting kept outside summaries; validate
actual serialized request gain; capture immutable projection provenance; and
qualify long-trace refresh, reopen and handoff. Current gain is content bytes,
not a provider-token measurement. No semantic summarizer, hidden helper request,
new model asset or installed CLI is claimed by this increment.
