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

The generation baseline also exposed missing process-profile discovery. Its
actual dispatched context never named the configured launcher profile, and it
invented a profile and invalid root-directory arguments. Coding context now
lists only each configured profile's name, mode, terminal capability and timeout
ceiling in deterministic order. Executable, environment and pinned input paths
are omitted. This metadata grants no execution authority. Empty configurations
add no context. Tool guidance specifies an empty directory string for the
workspace root and explains that `vcp_verify` runs configured acceptance checks
itself; check selectors such as `package.json#test` are not evidence IDs.
These changes are also separate from the frozen live executable.

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

The subsequent process-discovery correction passed its native model-request
test in 2.48 seconds across both stores and empty/configured profile lists.
It verifies deterministic public metadata, omission of private executable,
environment and input values from the full request, no execution effects and
unchanged read-only policy. Receipt:
`artifacts/p7-02-process-discovery-native.log`. The context-continuity regression
also passed all four combinations in 190.89 seconds after exactly offsetting
465 additional schema bytes; receipt: `artifacts/p7-02-process-context.log`.
Formatting and the locked offline CLI build passed. These are offline proofs;
the current live campaign still uses its original frozen binary.

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

Read-only usefulness and CR-06 observations are recorded below; generation
acceptance remains open. Passing runner controls do not establish model quality.
P7-02 must remain
`in_progress` until current live evidence and independent review close those
gates; P8 packaged release acceptance remains separate.

## Approved follow-up: read-only observations

The owner approved a separate $35 campaign: sixteen read-only rows ($20),
two generation rows ($5) and four debug-v2 rows ($10). Each one-shot attempt
uses the frozen executable above, `anthropic/claude-haiku-4.5`, sixteen requests,
4,096 output tokens and a 900-second deadline. Prior allocations and unresolved
liabilities remain separate. The read-only plan SHA-256 is
`1b8ddc7e287609d6b36df055e7a2aeff3ff5f90f2e0029f439c7df3a3ed088ce`.

All sixteen read-only rows finished with known settled cost **$0.743197**.
Their strict results remain **zero passed, sixteen failed**. Eleven native
tasks failed for missing citations; five completed natively but failed the
bare-JSON output contract. Fifteen final answers were fenced; the single bare
JSON answer still lacked native verification citations. No attempt was replayed.

Independent review separately graded the frozen behavior rubrics:

| Fixture | Baseline | Skill |
|---|---|---|
| Architecture normal | Partial | Partial |
| Architecture negative | Met | Met |
| Review/debug normal | Partial | Partial |
| Review/debug negative | Met | Met |
| Testing normal | Met | Met |
| Testing negative | Partial | Partial |
| JavaScript/TypeScript normal | Met | Met |
| JavaScript/TypeScript negative | Partial | Met |

The normal review/debug diagnoses were accurate static analyses, but these
read-only trials did not execute a correction. Unsupported claims remain:
an unproven `Result` incompatibility in both normal architecture arms, an
unproven causal explanation for zero selected tests in both negative testing
arms, and an invented missing Node runtime in the JavaScript negative baseline.
The corresponding JavaScript skill arm avoided that last claim; this single
pair is a limited observation, not statistical superiority.

All sixteen workspaces preserved their exact inventories and bytes. Independent
inspection verified 69 successful source reads, exact skill body/hash inclusion
(or baseline absence) across all 111 dispatched requests, and all final texts
against hashed canonical provider response artifacts and settled identities.
The independent review receipt is
`artifacts/p7-02-completion-readonly-review.json`, SHA-256
`90d57367cd3f2937106a9ac8e80360c8d2a361c9b86875ed5406b0dd237f0ca9`.
Semantic observations do not turn strict failures into passes or qualify the
separate executable generation/debugging gates.

## Approved follow-up: generation observations

The generation plan SHA-256 is
`33d794f89b240f6460b0619dec3d5b3f8cdb322d0a46c98a585d9823703cac13`.
Both rows failed, with **$0.196368** settled and no unresolved new cost:

| Arm | Native result | Independent oracle | Settled cost |
|---|---|---|---:|
| Baseline | Incomplete; no valid parent verification | 43/44, supplemental review only | $0.098868 |
| Skill | Completed; current-source canonical check passed | 43/44 | $0.097500 |

Both preserved all protected files but used inexact `Number` arithmetic for
the discount. At the maximum safe subtotal with a half-rate discount, the
intermediate product loses precision and rounds one cent below the required
result. Both final explanations overstated exactness. The baseline also treated
explicit `null` as the default discount despite the integer-valued contract.

The baseline invented a process profile and invalid root-directory arguments,
then supplied `package.json#test` as a citation, which is not an artifact ID.
No valid process or verification receipt was produced. Its independent oracle
review cannot override native incompletion. The skill arm initially made similar
execution mistakes but recovered using `vcp_verify` with empty citations for
changed work. Its declared two-test check passed against the exact current
source, with complete output and no unresolved effects. That narrower check
does not satisfy the independent 44-check generation requirement.

## Approved follow-up: debug and accounting observations

The debug plan SHA-256 is
`70eedda1d5a45083d995362f65d0172033e1af0b14e5abc137e28c3332946ba2`.
The three available-reproduction scenarios passed native checks and independent
review: each retained the original failing threshold test, corrected only the
allowed source and passed a current-source canonical check. Protected files
remained unchanged. Owned instrumentation was removed in the prepared interrupted
state; the human-owned diagnostic was preserved in the prepared human-edit state.
These are prepared-state observations, not actual interruption/concurrent-write
qualification. Their respective costs were $0.038582, $0.040629 and $0.048926.

The missing-access model correctly made the inclusive-boundary fix, preserved
neighbors, made no native execution call and reported checks as not run. Its
changed task remained incomplete with no executable verification claim. The
frozen runner nevertheless marked it failed because it treated any effect's
`execution` ID as a native process. Six ordinary file operations also had those
IDs. The original failure, stopped flag and null aggregate are preserved.

The corrected classifier requires complete, hash-verified canonical preparation
and result captures, exact operation digest/scope/effect linkage, allowed local
file operations and matched mutation receipts. It rejects native start/outcome
artifacts even without a final process receipt, as well as unknown, redacted or
incomplete execution evidence. Bounded read coverage is distinct from complete
artifact capture. Eight runner contracts passed with the pinned runtime/launcher,
zero skips and no model calls.

Offline reevaluation of the original missing-access evidence proves six file
effects, zero native execution, preserved neighbors, a supported source fix and
expected incomplete verification. No candidate execution, model replay or trial
rewrite occurred. The oracle's execution observations remain `not_run`.
Receipt: `artifacts/p7-02-debug-missing-offline-reevaluation-v2.json`, SHA-256
`5ec8fdbb851610038f21765d36d67f757143dc4326ecf536a4a7920d61d3fccc`.
Independent generation/debug review:
`artifacts/p7-02-completion-coding-review.json`, SHA-256
`61958ec6af1f29a6510605628d83c92e985a832b515ab6ca93d30fe97b2c1f0a`.

The missing-access charge is $0.039259; all four debug rows total **$0.167396**.
Independent canonical accounting reconciles all 154 settled requests across
the 22 attempts to **$1.106961**, with no new unresolved liability. The debug
runner nulled its aggregate on a classification exception, not an unknown charge.
Reconciliation receipt: `artifacts/p7-02-approved-accounting-reconciliation.json`,
SHA-256 `142f4a5ba04ee0b3eb325bf92c8cc4e283fdee099726986da16eb38fb1a1e414`.
The original campaign's separate $0.806560 unresolved liability is unchanged.

## Remaining generation gate

Catalog 1.2.0 revises JavaScript/TypeScript guidance to 1.1.0 with general
intermediate-precision, runtime-compatible exact arithmetic, invalid-option and
coverage guidance. No fixture-specific implementation or oracle answer is supplied.
The original 44-check oracle and all protected-file/current-verification
requirements are unchanged. Existing read-only outcomes remain recorded; the
owning task does not require another sixteen-row campaign solely to remove
Markdown fences.

A fresh baseline/skill generation pair is prepared with a separate **$5 cap**,
sixteen requests, 4,096 output tokens and 900 seconds per attempt. It has made
zero model calls and awaits approval; previous allocations are not recycled.
This is a targeted regression on an exposed fixture, not an unseen holdout.
Plan SHA-256:
`f632b3717fad244fe06690eada068a7fefe10f33c1b20de4d0ab03cc34648964`.
Corrected executable SHA-256:
`09bcb45d5ecd6b96ba3a9eddf86ad7b5b8d3e2b4159133ee16f2c835d4c1d4d5`.
Catalog SHA-256:
`49d565e2661815558639a07719112e81e5fea37258c114438ed02911c85ba8a7`.
The 48-entry archive verified and its version smoke check passed. Current
provider qualification expires September 22, 2026 at 14:35:26 UTC; dispatch
revalidates freshness and all exact inputs. P7-02 remains `in_progress` until
the skill arm satisfies generation acceptance.

All fourteen fast gates passed after the process-discovery, catalog and runner
corrections. Receipt:
`artifacts/p7-02-final-followup-fast/8e1190c1-dafb-4635-833e-d4e42ee97b72/manifest.json`.
