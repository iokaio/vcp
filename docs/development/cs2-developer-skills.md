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

## Remaining sequence

1. **In-run checker.** A data-only `vcp-developer-check` binary with a build
   receipt, and feasibility tests with a synthetic provider. The feasibility tests
   cover the ceiling-bound tools, checker completion, the two-skill MCP nearest arm,
   explicit candidate-source selection, and a reference-size MCP server within
   2,048 output tokens.
2. **Campaign tooling.** Developer candidate sources, preparation, a one-shot
   runner, reader packets and a promotion script.
3. **Campaign.** A provider refresh, then preparation and preflight bound to the
   envelope SHA, then three eighteen-run blocks. After that come grading, blind
   reading and per-skill decisions.
4. **One PR per skill,** in the order `llm-integration`, `mcp-development`,
   `frontend-design`. A qualified skill is promoted; otherwise the non-default
   disposition applies.

Explicitly not run until observed:

- installed-SDK and live provider or MCP compatibility;
- browser, DOM, keyboard, focus, viewport, accessibility and reduced-motion
  checks (CS-3);
- visual review and human review;
- Linux and macOS;
- general statistical benefit.
