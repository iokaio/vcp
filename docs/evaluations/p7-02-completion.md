# P7-02 completion qualification

Status: in progress. This increment addresses the failed September 21 live
campaign without changing its results, denominators or canonical liabilities.
The original eighteen attempts remain ten failures and eight not-run cases;
$0.051188 is settled and $0.806560 remains unresolved. Original allocations are
not available to fund replacement trials.

## Corrected evaluation and tool guidance

The P7 runners now have their own finite limits: at most sixteen requests,
1,800 seconds and 8,192 explicitly requested output tokens, within the current
qualified provider maximum. P6's frozen 512-token smoke limit is unchanged.
The old generation attempts exhausted 512 output tokens with incomplete patch
arguments; a fresh coding trial can request 4,096 tokens. Every plan still binds
the exact executable, catalog, fixtures, profile and runner sources, with zero
transport retries, per-attempt allocations and one-shot execution claims.

Provider, compatibility and price expiry must all be current. Read-only trials
retain plan authority; generation source profiles retain workspace read/write
authority and require an explicit proposal for the restricted verification
launcher. Fresh campaigns require separately approved caps and cannot replay
old attempts or reuse their allocations.

Read-tool guidance now explicitly distinguishes relative file paths from
artifact IDs and requires both nullable line-bound fields. Patch-tool guidance
includes the actual literal patch syntax. Parsing, source-version checks and
authority remain unchanged. Read-only trial prompts explicitly request bare
JSON; historical XML/prose-wrapped answers remain failures.

The approved follow-up campaign exposed an unchanged-analysis usability gap:
both normal architecture arms read the relevant files but supplied empty
verification citations. The host correctly refused completion. Subsequent
product guidance explicitly maps successful tool-result `evidence` UUIDs to
`citations`, explains the non-finalizing `complete:false` field and directs
the caller to resolve outstanding issues. The missing-citation diagnostic
gives the same recovery action. Explicit citation selection, source/scope
validation and completion requirements remain unchanged. These edits are not
part of the frozen executable used by the approved campaign; its failures
remain failures.

## Evidence so far

- Eleven native tool preparation tests passed, including scope/version fences,
  exact patches, ranged reads and search cancellation.
- Twenty-nine live-runner, generation and historical-continuation contract tests
  passed with the pinned Node 26.9.0 and recorded verification launcher, with
  zero skips or model calls. The local receipt
  `artifacts/p7-02-completion-runner-contracts.json` is explicitly an observed
  output summary, not a raw log.
- Seven native-toolchain contract tests passed. The additional bounded data
  recipe and full native matrix are recorded in
  [the toolchain evaluation](p7-02-native-toolchains.md#data-contract-extension).
  Unavailable host/toolchain rows remain unvalidated.
- Nine debug-runner and preserved debug-v1 controls passed using the pinned
  Node runtime and newly built restricted launcher, with no skips or model calls.
- The real native CLI debug-v2 fixture passed in 36.54 seconds using that launcher
  and a localhost synthetic provider. It proves test discovery with only
  `shipping.cjs` declared affected, a failed original reproduction, successful
  current-source canonical verification after the fix, protected-file
  preservation, and exit 3/incomplete acceptance when execution is unavailable.
  Receipt: `artifacts/p7-cr06-native-cli.log`. This is executable integration
  evidence, not live model usefulness.
- The native context-continuity regression passed all four normal/oversized and
  file/SQLite combinations in 206.05 seconds. Its initial attempt failed because
  the added tool descriptions consumed exactly 372 serialized bytes of its
  fixed envelope. A fixture-only 372-byte adjustment restores the original
  history budget; every compaction, original-preservation, current-fact,
  unknown-cost and oversized-request refusal assertion remains intact.
  Receipts: `artifacts/p7-02-completion-context.log` and
  `artifacts/p7-02-completion-context-retry.log`.
- The locked offline Rust 1.98 CLI build passed with existing warnings. Its
  48-entry executable/skill archive verified successfully; executable SHA-256
  `e6add2b99847df753abbd861014a06925c4a8bbf977fccde4a34b2004325ba30`.
  The staged executable also passed its help-command smoke check. This is
  sidecar/archive evidence, not P8 distribution qualification.
- All fourteen repository fast gates passed as the repository owner. Initial
  sandbox attempts could not satisfy Git ownership checks; the harness strips
  transient Git configuration, so the successful run used the owner context
  without changing global Git configuration. Receipt:
  `artifacts/p7-02-completion-fast/f0678c6b-a287-4351-9d62-007ea1c46458/manifest.json`.
  After adding the debug-runner tests to the permanent built-in-skills gate,
  that updated gate also passed:
  `artifacts/p7-02-completion-fast/cd5f3ff2-be60-44f7-9455-6c36b8c6cdea/manifest.json`.

The citation-guidance correction passed the existing native verification
regression across both stores (187.21 seconds) and all four context-continuity
combinations (192.49 seconds), with no model calls. Its additional 305 schema
bytes are offset exactly in the compaction fixture, preserving the source/history
budget and all refusal assertions. Receipts:
`artifacts/p7-02-citation-verification.log` and
`artifacts/p7-02-citation-context.log`. Rust formatting and the locked offline CLI
build passed; the corrected 48-entry archive verified and passed its help smoke
check. Corrected executable SHA-256:
`8f40b767e94c639f3bc817c88de932d830c4f5114c89e0487669b24f035cdc59`.
This is offline validation, not a new live qualification.
All fourteen fast gates also passed for this correction; receipt:
`artifacts/p7-02-citation-fast/b77f1917-7a73-4267-8c1b-1195fb3d2cf6/manifest.json`.

## Fresh live acceptance

Use the frozen per-case behavior rubrics for the sixteen read-only observations
(baseline/skill pairs for architecture, review/debug, testing and JS/TS).
Independent review must check actual source references, seeded findings, false
positives, project-specific check recommendations and truthful not-run claims.
Negative-case abstention without inspecting the relevant evidence is insufficient.
All failed and not-run attempts remain in the comparison; a passing baseline is
not evidence of added skill benefit, and no statistical superiority is claimed.

The generation skill arm must implement the requested feature, preserve all
protected files, pass current canonical parent verification and satisfy all
44 independent oracle checks. Review the baseline separately against the same
criteria and retain its actual outcome.

CR-06 uses a new debug-v2 fixture with a frozen executable threshold test because
debug-v1's package had no test script. The v1 fixture and oracle remain unchanged.
Available-reproduction cases require a captured failure before the edit, a
passing check bound to the current source, canonical parent verification and
independent final-file/oracle review. Missing-access must retain no execution
authority, leave changed work incomplete, preserve neighboring files and report
verification as not-run. Independent review must assess the supported fix and
the final explanation; preservation alone does not establish fix correctness.
Prepared instrumented and human-edited states qualify those skill scenarios,
not actual process interruption or concurrent writes.

Live usefulness, generation and CR-06 scenario acceptance are still pending.
Passing runner controls do not establish model quality. P7-02 must remain
`in_progress` until current live evidence and independent review close those
gates; P8 packaged release acceptance remains separate.
