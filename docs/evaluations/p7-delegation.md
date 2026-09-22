# P7 delegation qualification

P7-04, P7-05 and P7-06 remain in progress. The implemented owner path is described
in [child workspaces](../development/p7-child-workspaces.md). M7 remains deferred.

The [2026-09-22 paired campaign](p7-live-qualification-2026-09-22.md) adds current
native regression, actual Luna/Qwen review and generation observations, and live
terminal evidence. The earlier mock-provider results below remain historical
component evidence, not substitutes for that campaign's failed or open gates.

The [P7-05 quality retest](p7-review-generation-quality-2026-09-22.md) corrects
evidence-reference grading and qualifies Luna's revised review guidance. Qwen's
paired usefulness gate remains open; provider failures and invalid tool arguments
are retained separately from review quality findings.

The native campaign uses `scripts/test-delegation.ps1` with Rust 1.95.0 and the
installed Visual Studio x64 tools. Its manifest binds source inputs and stage
logs, and refuses a passing result when inputs change during the run. All model
traffic in these fixtures goes to a local mock server; no live usefulness or
provider-cost claim follows from a passing fixture.

| Boundary | Independent observations |
| --- | --- |
| Graph and budget | Atomic child/allocation creation on both stores; bounded ancestry and concurrency; dependency and scope checks; cancellation retains history; exhausted root/child exposure blocks admission without counting unused allocations as charges. |
| Workspace | Staged, working and explicitly selected untracked bytes; unchanged parent; native directory identity; non-Git isolated copies; cancellation after the first copied file prevents subsequent copying and ready publication. |
| Dispatch | Real retained child with exact assigned model, canonical root accounting, isolated reads/patches, protected ownership marker and parent pause fencing; mismatched model causes no wire request or reservation. |
| Integration | Observed child changes, three-way conflicts, ordinary parent broker, concurrent human edit rejection, read-only findings retained, and no child result bypass of current parent verification. |
| Partial effect | A Windows sharing lock permits the first integration write and rejects the second. Per-file receipts and unknown outcome persist. Reconciliation artifacts identify the exact effect/execution, report no replay and preserve later human edits. This is native failure evidence, not control cancellation between writes. |
| Recovery | Fresh owners on both stores preserve child edits and held siblings; moved/missing workspace diagnostics; abandoned startup tickets pause unbound children; explicit current resume evidence required. |
| CLI | Actual PTY delegation and public transcript capture; pause with root and child requests active; inspection remains available with no extra send; live-owner JSONL returns the same child projection; stale terminal events cannot stop a later turn. |

Earlier focused passing logs are retained locally under `artifacts/`, including
`p7-repository-tools-tests.log`, `p7-child-graph-final8.log`,
`p7-child-connected-final3.log`, `p706-recovery-provider-retry.log`,
`p706-cli-delegation-pause.log`, and `p706-cli-agents-jsonl.log`. These precede the
last review fixes. The final source-bound native run passed at
`artifacts/delegation/1342f831-7ead-4317-9823-350193a0cc9e/manifest.json`
(SHA-256 `a6e2264936a640aabf40f7d6a9e5fdbb31f8d313d2f02b60038788092fc4cde9`):
37 repository/tools, 98 canonical contracts, 8 connected child cases, 66 CLI
library cases and 2 native PTY cases passed; four existing tests remained ignored.
The final repository fast gate passed all 14 stages in
`artifacts/p7-delivery-final/b8434403-f227-4cb7-85cf-62370eff6a0e/manifest.json`.

A broader host run was interrupted and is not passing-suite evidence. It exposed
a stale coding fixture that asserted parallel calls without qualifying that
optional provider capability. The fixture now declares the capability and retains
the original assertions; its separate 16-case matrix passed in
`artifacts/p7-coding-capability-retry.log`. No production capability gate changed.

Remaining acceptance includes live U02/U03 usefulness, the remaining U06 multi-child
progress/consumer-loss campaign, control cancellation between integration writes,
and packaged P8 checks. Executable child processes remain blocked without a
qualified filesystem sandbox. Post-attachment setup failure requires owner
close/reopen recovery; it does not discard the workspace, history or allocation.
No automatic worktree deletion or release publication is included.
