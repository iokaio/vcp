# CS-2 developer specialists: implementation and qualification

Status: in progress. The unqualified 1.0.0 candidates `llm-integration`,
`mcp-development` and `frontend-design` are frozen for the campaign as
non-default packages in `src/skills/candidates/`, with the campaign tooling. No
model call, comparison or promotion has run. Contract:
[plan 24 § CS-2](../plan/24-skills-follow-on.md#cs-2--developer-specialists).

## Owner decisions (September 25, 2026)

- **Entry.** CS-2 implementation waited for CS-1 to exit. CS-1 closed by owner
  direction with non-default candidates (#172, #173, #175).
- **References are project-local.** Candidate packages keep `resources: []`,
  because activation loads every declared resource with the body. Skills direct
  the model to the user's installed SDK sources and project documents, and to
  record the exact versions and dates they used. Fixtures carry the
  project-specific references. The loader is unchanged.
- **Paid campaign envelope.**
  - Per run: at most USD 3 and 16 requests. Total: USD 162 and 864 requests for
    the fifty-four three-arm runs.
  - Up to two provider refresh probes, USD 1.50 in total.
  - CS-1's one-shot claim, per-run budget and stop-on-unknown-charge rules apply.
  - The envelope has no headroom: any repeat, retry or extension needs new
    approval. It does not consume P8 or CS-1 allocations or clear unknown charges.
- **Process authority, for pinned identities only.**
  - the data-only in-run developer checker, identified by its build receipt;
  - the AppContainer adapter runner, bootstrap and grader, identified by hash;
  - installed Node 24.21.0, identified by SHA-256
    `ba4e6d110e8c1592a1ecd390f6b05f3da124b13871a5be62b341a07a853c6c32`.

  Dispatch refuses a changed identity, and any change needs new approval.
- **Readers.** Two independent blind agent readers score every case and arm.
  Human review is recorded `not_run`.
- **Benefit rule, predeclared.** A candidate benefits on a normal task in either
  of two ways:
  - it passes executable checks that both baselines fail, with reader scores no
    lower; or
  - it earns at least +1 reader usefulness.

  In both cases there must be no correctness or preservation regression. Ties
  remain unqualified.
- **Exit per skill.**
  - A qualified skill is promoted to the default catalog in its own PR.
  - `frontend-design` may be promoted in CS-2. CS-3 re-grades its retained
    artifacts in a real browser, and a failure there reopens it.
  - An unqualified skill stays a non-default candidate with recorded gaps. It
    closes only by explicit owner acceptance.

## Delivered prerequisites

| Increment | Evidence |
|---|---|
| Single-shot Windows Node fixture adapter (#177) | [Adapter record](cs2-node-fixture-adapter.md) |
| Canonical model tool ceiling (#178) | [Tool ceiling](p2-canonical-tool-ceiling.md) |
| Interactive adapter mode (#179) | Parent-owned transports, iterators and protocol peers over bounded relayed frames; fourteen native cases |
| Developer fixtures v5 and parent-side grader | [Fixture inventory](../../src/evals/skills/developer/README.md), [design](cs2-fixture-design.md) |
| In-run developer checker and synthetic feasibility (#181) | [In-run checker](#in-run-checker) |
| Campaign candidates, preparation, runner and blind review | [Campaign tooling](#campaign-tooling) |

The v5 grader verification on native Windows 10.0.26200.0 x64 with the pinned
Node:

- The portable structural and history suite passed (11 tests).
- The probe suite passed (15 tests):
  - Every one of the eleven graded cases passes its trusted reference.
  - Every recorded regression fails. The regressions include per-chunk and
    UTF-16 stream limits, promise results from synchronous contracts, and server
    state outside the caller's object.
  - Contract-consistent variations of unstated details pass.
  - Harness faults leave the verdict open for regrading.
- The AppContainer grading test passed. For each of the six wrapper kinds, a
  reference double passed and a regression failed, and no profile leaked.

An independent review found three verdict-affecting grader defects and several
should-fix items; all were corrected before any campaign identity was pinned.

Grader error messages may quote candidate output, so they stay out of
blind-reader packets. No model artifact has been graded.

## In-run checker

`vcp-developer-check` (feature `qualification`) is the only process a campaign
run may start. It is data-only: it never executes workspace content.

- **Pinned inputs.** It embeds the v5 manifest and the thirteen write-case
  oracles. Every case it resolves, including every owner-map entry, must match
  its manifest hash; a unit test checks all thirteen.
- **Fixed invocation.** The verifier must invoke it with exactly
  `--test --test-reporter=tap --test-concurrency=1 checks/developer.test.cjs`,
  and the scaffold must hold an inert marker file.
- **Owner case map.** It reads the case identifier for the workspace from
  `developer-cases.json` beside the executable, which the preparer writes. The
  map holds one entry per write-case run, at most thirty-nine: thirteen write
  cases on three arms. Report-only runs configure no checks and never appear in
  it; a report-only entry fails every lookup. The model cannot choose its own
  case.
- **Bounded discovery.** At most 128 entries, 64 KiB per file and 1 MiB in total.
- **Two TAP tests.**
  - `developer input preservation`: every non-editable fixture file and both
    scaffold files are byte-identical.
  - `developer output structure`: no extra files; every editable file exists
    within 64 KiB (256 KiB in total); HTML asset references stay local and
    exist. The HTML scan follows the parent oracle exactly. A shared corpus,
    `src/tests/fixtures/developer-html-assets.json`, holds expected verdicts
    that both suites assert.
- **Build receipt.** `scripts/evals/developer-check-build.ps1` builds the
  binary offline with the locked dependencies. It writes a
  `cs2-developer-check-build/1` receipt holding the executable, source, fixture
  manifest and builder hashes; the toolchain; the hashed source inputs; and
  whether those inputs were unchanged across the build. The campaign pins the receipt that preparation builds from
  the merged commit. Local build receipts are verification evidence only.

The checker does not grade functional behaviour. That stays with the
parent-side AppContainer grader after the run.

### Synthetic feasibility

Two `vcp-cli` executable tests (`developer_feasibility`) run the real CLI
against a scripted provider. They pass on native Windows with the installed Node
24.21.0 and the built checker.

- **One write case per prefix completes, each on a different arm.**
  - UI (`UI-near-miss-parser-v2`) runs on the none arm.
  - MCP (`MCP-near-miss-rest-v3`) runs on the nearest arm, which activates
    `architecture` and `javascript-typescript` together.
  - LLM (`LLM-near-miss-parser-v2`) runs on the candidate arm. It uses a probe
    package selected from an explicit user source, the same mechanism the
    campaign uses for the unpromoted candidates.
- **What each write case asserts.**
  - The tools advertised on every request equal the case's `canonical_tools`.
    A call outside the ceiling fails dispatch admission; the
    [tool-ceiling](p2-canonical-tool-ceiling.md) tests cover that.
  - The activated skill bodies match the arm.
  - `vcp_patch` changes the editable file.
  - `vcp_verify` runs the checker, which reports `ok 1` and `ok 2`.
  - Every other fixture file is preserved.
- **Report-only completion.** `LLM-hostile-diagnostics-v2` completes without any
  process. Its `vcp_verify` call cites the evidence from a `vcp_read`, and
  completion requires that citation.

**Output-token estimate.** Output tokens were estimated, not measured with a
tokenizer. The largest trusted reference is the `MCP-normal-resources-v2`
server, at 2,254 bytes. Rewriting the whole file takes one `vcp_patch` argument
of 2,486 JSON characters, including the envelope, the removed stub lines and the
line prefixes. At a conservative 3 bytes per token that is about 830 tokens, so
a whole-file write fits in 2,048 output tokens with about 2.5 times headroom.

- **Second request.** The prompts also ask the final answer to return the
  written contents, so a write run spends about the same again. That comes in a
  separate request, and each request has its own 2,048-token limit.
- **Reasoning tokens.** The profile's `output_tokens` limit includes reasoning
  tokens, and the estimate does not account for them.

Only the live campaign can confirm the estimate.

## Campaign tooling

**Candidates.** `src/skills/candidates/{llm-integration,mcp-development,frontend-design}`
are version 1.0.0, with one explicit cue each and no resources. Before freezing,
each package received its one allowed body edit (D4):

- `frontend-design`: WCAG 2.2 AA contrast guidance, and a written brief for a new
  interface.
- `mcp-development` and `llm-integration`: they bundle no references and send the
  model to the project's installed SDK sources and supplied documents, recording
  exact versions and dates.

`scripts/evals/developer-candidates.cjs` registers them as the explicit user
source `vcp-developer-candidates`. It returns each arm's selections as an array,
so the MCP nearest arm activates `architecture` and `javascript-typescript`
together through repeated `--skill`.

**Preparation.** `scripts/evals/developer-prepare.cjs prepare <spec.json>
<new-private-directory>` makes no process, provider or credential access.

- **What the spec must name:** the envelope (USD 162, 864 requests); the
  checker build receipt; the grading Node, whose hash must be the owner-approved
  `ba4e6d11…`; and an explicit checker-process proposal.
- **Profile requirements.** The source profile must be qualified and currently
  valid, with 16 requests and 2,048 output tokens per run.
- **What it stages:**
  - fifty-four runs in three per-skill blocks, in campaign order, with each
    case's arm order rotated;
  - the frozen project and checker scaffold for each run;
  - the fixture prompt, byte-for-byte;
  - a derived profile bound to the case's `canonical_tools`;
  - the staged checker, and an owner case map holding the thirty-nine write runs.
- **What the plan records:**
  - the source identity, which covers the candidates, fixtures, checker sources,
    grader and adapter files and the CS-1 helpers it imports;
  - the executable and packaged assets;
  - the pinned checker, grader and Node identities;
  - the first-request budget preflight;
  - the predeclared benefit rule.

**Runner.** `scripts/evals/developer-runner.cjs run <plan.json> <plan-sha256>
<block>` executes one block, once, in campaign order.

- **Before every dispatch** it:
  - re-derives the whole preparation and compares it exactly;
  - reserves a full USD 3 and 16-request slot from retained settled accounting;
  - writes a run claim.

  It also holds one campaign claim in the Git control directory, so a second
  preparation cannot draw on the same authorization.
- **After each run** it:
  - reconciles canonical costs;
  - rejects writes outside the editable paths;
  - checks the context of every settled request against every part of each
    selected skill;
  - checks that the pinned checker ran;
  - applies the structural oracle;
  - scans retained model output for the synthetic canary.

  The checker check needs a passed `package.json#test` check whose retained
  outcome records the pinned executable hash and the fixed arguments, and whose
  retained stdout carries both TAP lines. The feasibility test asserts this
  evidence shape against the real CLI.
- **What stops the campaign.** An unknown or unreconciled charge, identity drift
  or an expired qualification, an authority failure, or an interrupted block. Any
  of these writes `halt.json` for read-only reconciliation. A disclosed canary or
  a failed check fails only its case.

**Review.** `scripts/evals/developer-review.cjs` runs in three steps:

1. **`grade`** grades each unstopped block's retained write artifacts with the
   pinned AppContainer executor. Probe messages go to a private diagnostics file.
2. **`packets`** writes one anonymous packet per case, holding:
   - the prompt and the frozen sources;
   - each variant's edited files, report and not-run list, under a random label;
   - check verdicts only.

   Selection names are replaced, and the label mapping stays private.
3. **`decide`** validates two independent blind reviews and applies the mapping.
   - A recorded effect beyond authority, or a real secret exposure, halts the
     campaign.
   - A skill qualifies only when all six candidate runs complete and pass every
     executable check with both readers passing every hard gate. It also needs a
     benefit on a normal case under the predeclared rule.
   - Any grading that requires a regrade leaves the decision pending.

## Remaining sequence

1. **Campaign.** A provider refresh (at most two probes, USD 1.50), then
   preparation and preflight, then the three blocks. After each block come
   grading, blind reading and the decision.
2. **One PR per skill,** in the order `llm-integration`, `mcp-development`,
   `frontend-design`.
   - A qualified skill is promoted to the default catalog. The promotion script
     and package qualification are built with the first skill that qualifies,
     not before.
   - Otherwise the skill stays a non-default candidate.

Explicitly not run until observed:

- installed-SDK and live provider or MCP compatibility;
- browser, DOM, keyboard, focus, viewport, accessibility and reduced-motion
  checks (CS-3);
- visual review and human review;
- Linux and macOS;
- general statistical benefit.
