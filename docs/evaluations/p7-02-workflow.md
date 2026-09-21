# P7-02 developer workflow qualification

This increment implements the selected CR-02a/CC-04, CR-06, CC-02a and CC-03
refinements within P7-02. Its prerequisites P7-01 and P2-06 are recorded complete
in the task ledger. The starting revision is `c49f677`; evidence below applies
to the resulting working-tree changes, not that starting commit alone.

## Changed boundaries

- Bounded regex/path search and inclusive ranged reads use existing native roots,
  version probes, preparation, policy and broker receipts. Default literal search
  and whole-file reads retain their previous request/result behavior. New schema
  identities invalidate old preparations. Pause/steering cancellation reaches
  discovery, probe revalidation and both broker read-release checks.
- Trusted profile and check durations share the existing finite one-hour task
  envelope. Omitted settings retain 120 seconds. Preparation, policy admission,
  the native supervisor and dispatch deadline checks enforce their respective
  bounds; project/model input cannot increase trusted profile limits.
- Pipe and PTY presentation retain actual output tails. Explicit UTF-8/UTF-16LE
  decoding records omission and loss while keeping raw output artifacts, stop
  reasons and exit status authoritative. This adds no executable-name-based
  outcome reinterpretation.
- Catalog 1.1.0 updates the architecture, review/debug and testing procedures,
  descriptors and hashes. Guidance addresses source ranges, current review scope,
  causal uncertainty, owned instrumentation, concurrent human edits and honest
  verification results.
- Generation checks include previously omitted invalid-input controls. A separate
  CR-06 fixture/oracle checks final files, original/fixed threshold behavior,
  instrumentation cleanup and preservation of human edits. Neither fixture
  controls nor successful packaging establish live skill usefulness.

The development contract is documented in
[the catalog guide](../development/p7-builtin-skills.md). ADR-024 and ADR-025
continue to govern activation, source identity and packaging. The existing regex
dependency is reused; the sole vendored change is the workspace lock dependency
edge, retained as patch `0036-p7-navigation-workspace.patch`.

## Verification record

Native verification used Rust 1.98.0, the locked offline workspace and the
`x86_64-pc-windows-msvc` target on September 21, 2026. Local receipts are retained
under ignored `artifacts/`; they are not published CI artifacts.

| Boundary | Observed result | Local receipt |
|---|---|---|
| Repository fast gates | 14 passed | `p7-02-delivery-fast/a99b9345-ef8e-4c2f-a8b1-023779831626/manifest.json` |
| Navigation and schemas | 16 tools, 16 provider and 8 repository tests passed | `p7-02-navigation-evidence.json` (summary of tool output, not raw logs) |
| Catalog and frozen skill fixtures | 4 catalog tests and 42 fixture cases passed, zero model calls | `p7-02-catalog-final.log`, `p7-02-builtin-qualification.json` |
| CLI unit / contracts | 67 passed, 1 external-cloud test ignored / 8 passed | `p7-02-cli-unit.log`, `p7-02-cli-contracts.log` |
| CLI executable workflows | 20 passed | `p7-02-cli-native.log` |
| Retained coding, verification, instructions and skills | 6 passed initially; the context case passed its focused rerun after fixture recalibration | `p7-02-retained-loop.log`, `p7-02-context-continuity-recalibrated.log` |
| Actual foreground checks over 120 seconds | Passed execution and verification on both file and SQLite stores, including reopen | `p7-02-long-native-retry.log` |
| Process broker and framework checks | 2 passed, including PowerShell output, cancellation and seeded failure/fix | `p7-02-process-framework-native.log` |
| Extracted archive | 2 native tests passed; 48 packaged entries verified | `p7-02-archive-native.log`, `p7-02-workflow-package/db39f1ed-f936-4787-aa44-c25e3ad65c3d/result.json` |
| Debug/generation/live-runner controls | 20 passed, zero skips or model calls with pinned Node 26.9.0 and fresh recorded launcher build | `p7-u03-launcher-6970e56acc344e8797e73a895443e740/qualification-contracts.log` |
| Static checks | Clippy completed for all targets of six affected crates; style warnings remain. Changed Rust formatting and diff whitespace checks passed | `p7-02-clippy.log` |
| Vendored reconstruction | All 7,939 inventoried files matched | `p7-02-codex-reconstructed.json` |

The packaged executable SHA-256 is
`1be77e70b3a6572b2e28576721802e82fae86cf00462aa931d7758d6d062e72c`;
the catalog SHA-256 is
`974cc43a3e3730c967b20cd5fa00810383a72346d88a9fed27831603ad92ac94`.
After the full executable run, the new inspector test was narrowed to the two
process artifacts identified by durable event facts. Its fresh-process raw-byte,
decoding and failed-verification assertions passed again in the focused rerun
(31.46 seconds; captured tool output).
Development attempts, including the initial long-check provider setup and
process-reopen failures, remain in their original logs. These harness defects
were corrected before the successful reruns; no failed application check was
reclassified as passing.

The retained-context fixture initially paused before its expected seventh
request: its 23,488-byte effective envelope could not fit the 23,563-byte compacted
projection after tool schema growth. Its private provider fixture now adds 1,000
bytes of prompt capacity, offsetting the 891-byte schema increase and explicit
nullable read arguments. The shared provider fixture and production limits are
unchanged; forced compaction, original-artifact preservation, unknown-cost and
oversized-context refusal assertions remain intact.
The focused test passed all four store/oversized combinations in 190.72 seconds.

## Remaining acceptance

P7-02 remains in progress until the live U01–U03/U08 usefulness and remaining
toolchain requirements have current evidence. No P6-specific spending authority
is reused for P7 trials. Live plans bind the final executable, catalog, profile,
fixtures and aggregate spend cap before dispatch. Missing environments and
seeded failing application checks retain their previous status; they are not
converted into passes by this increment.

U03 preparation now exposes the broker's conservative seven-effect process
classification in an explicit permission proposal. Only the pinned,
argument-restricted verification launcher is configured; no other process, MCP
or routing is admitted. Without that proposal the prepared profile remains
workspace-only and non-runnable. Exact-plan permission and spending approval
remain required. A fresh launcher build receipt binds the observed compiler,
source, embedded runtime paths and binary; it is not a third-party attestation.

The debug oracle's interrupted case starts from a frozen instrumented state;
it is not evidence of a real interrupted model session. Its process permissions
bound external access but do not authenticate hostile candidate computation;
independent source/output review remains necessary. Packaged Windows release
qualification and distribution remain P8 responsibilities.
