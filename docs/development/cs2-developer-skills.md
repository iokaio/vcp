# CS-2 developer specialists: implementation and qualification

Status: in progress. Unqualified 1.0.0 drafts of `frontend-design`,
`mcp-development` and `llm-integration` are preserved outside `main`. They return,
frozen for the campaign, with the campaign tooling and their per-skill PRs. No
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
| In-run developer checker and synthetic feasibility | [In-run checker](#in-run-checker) |

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

### Review hardening

The follow-up review increment closes gaps in the fixture prerequisite:

- Artifact contents must survive UTF-8 staging unchanged and contain no NULs,
  matching native checker discovery.
- UI transition probes cover every declared state/event combination; MCP
  initialization checks the JSON-RPC envelope; stream probes observe reads and
  reject consumption after cancellation or processing an aborted chunk.
- Invalid containment or cleanup receipts, incomplete runner envelopes and
  failed runner exits require regrading, including after a candidate failure.
- Windows children join their kill-on-close job atomically during creation,
  closing the owner-loss window before the first exchange.

Verification on the same native Windows host and pinned Node passed 38 portable
regressions, all three native grader/runner tests, fourteen containment cases,
and both synthetic CLI feasibility tests. The final session/grader corrections
passed their focused regressions and harness cases. The full fast harness passed
twenty cases; its protocol case initially failed because the new mock depended
on a filtered host variable, then passed after the mock supplied its own profile
directory. Syntax and diff-whitespace checks passed.

Earlier failed attempts remain in local evidence: sandbox account/profile and
Git-ownership mismatches, a missing feasibility Node setting, and the native
grader's cleanup assertion against an inactive synthetic profile from the prior
interrupted run. That exact profile was reconciled before the native rerun;
candidate assertions and cleanup then passed without relaxing the checks.

This increment does not complete specialist qualification or authorize changed
campaign process identities. Campaign tooling and skill acceptance remain pending.

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
a whole-file write fits in 2,048 output tokens with about 2.5 times headroom. The estimate does not
account for provider reasoning tokens that may count against the same limit.
Only the live campaign can confirm this.

## Remaining sequence

1. **Campaign tooling.** Developer candidate sources, preparation, a one-shot
   runner, reader packets and a promotion script.
2. **Campaign.** A provider refresh, then preparation and preflight bound to the
   envelope SHA, then three eighteen-run blocks. After that come grading, blind
   reading and per-skill decisions.
3. **One PR per skill,** in the order `llm-integration`, `mcp-development`,
   `frontend-design`. A qualified skill is promoted; otherwise the non-default
   disposition applies.

Explicitly not run until observed:

- installed-SDK and live provider or MCP compatibility;
- browser, DOM, keyboard, focus, viewport, accessibility and reduced-motion
  checks (CS-3);
- visual review and human review;
- Linux and macOS;
- general statistical benefit.
