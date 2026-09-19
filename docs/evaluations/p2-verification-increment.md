# P2 native verification increment

Status: this increment passed local qualification on September 19, 2026. P2-06 remains
`in_progress`; this report does not complete P2 or establish an installed product.
The [implementation guide](../development/p2-verification.md) defines the
qualified command subset and remaining acceptance limits.

## Change and native fixture

The canonical host discovers explicit project checks, captures their native
process and complete output receipts, records current/stale applicability, and
requires current owner verification for completion. The test configuration is
trusted input; model content cannot claim that a check passed. Original and
current source bytes, failed checks, changed paths and cost certainty remain in
canonical history.

Three native tests share a fixture covering fourteen scenarios per backend (28 total):
passing check, seeded failure, missing runner, no acceptance tests, wrong test
name, mid-check edit, later edit, analysis, changed analysis, a late child effect,
owner reopen, an ignored instruction edit, an uncovered path and invalid-executable
dispatch. The last scenario exercises dispatch
failure after durable intent. It requires failed/uncertain status, preserved
plan/effect correlation and no independent assertion marker.
It copies the installed Node executable to an owned temporary tool root and runs
actual `node:test` cases through the broker. An independent marker outside the
workspace records actual assertion execution. The task initially has
`editing=false`; changing its source still requires checks. The fixture also
checks uncited analysis rejection, fresh verification after a correction, and
retention of earlier failing results.

An initial native run passed the original twenty cases in 372.63 seconds. Later source review
added instruction absence/version probes and the universal raw-completion guard;
the complete integration qualification below validates those final inputs.
The first broad integration run was deliberately interrupted to add the fresh
workspace-effect fence: a terminal child can retain an unresolved effect without
advancing the root task revision. Its regression creates that canonical state
after an otherwise valid verification. The expanded fixture also reopens a fresh
owner in the same test process, rejects the old verification capability and
checks that an edit made while closed is compared with the original baseline.
A second broad run was deliberately interrupted for durable verification intent
and uncertain-dispatch classification. Neither interrupted run is acceptance
evidence; the final integration sequence passed with stable inputs.
A subsequent full run exposed the fixture's 15-second startup wait. The concurrent
edit case now uses an independent started/release file handshake instead of a
500 ms delay, with a separate bounded startup wait. Its focused native run passed
both backends in 115.32 seconds; the real process deadline and stale-completion
assertions remain enforced.
Pure discovery/parser tests cover shell/hook/glob rejection, missing targets,
incomplete discovery, explicit Cargo arguments, zero tests, wrong expected tests,
failure, timeout, duplicate summaries and filtered tests. A native 65 MiB
synthetic executable fixture verifies streaming identity, deny-write pinning and
stale replacement rejection without raising the source capture ceiling.

## Recorded commands

Native environment: Windows `10.0.26200`, NTFS, MSVC `14.50.35717`, Rust
`1.98.0 (88d9e12ae 2026-08-18)`. This remains the documented experimental
qualification toolchain; the original baseline pin is unchanged. Tests use
synthetic inputs, local stores and local Node execution, with no paid model calls.
The installed Node runner is `v24.10.0`, SHA-256
`93d6b1a22733fb035b19649041c0574b1a890df75ebce9d5e6e1bbe35d12b4a8`.

| Check | Result | Local manifest and SHA-256 |
|---|---|---|
| `pwsh -NoProfile -File scripts/test-tools.ps1 -Jobs 2` | Pass: 31 contracts and retained patch regressions | `artifacts/tools/398eb972-1aba-4352-abb8-a011c34022e5/manifest.json`; `e328704028886a2bf424d140cfdbae3d7d2be58279e252c9a64823746ffc0be2` |
| `pwsh -NoProfile -File scripts/test-integration.ps1 -Jobs 2` | Pass: 42 contracts, two retained launch regressions and seven-request private CLI trace | `artifacts/integration/93c45693-ef79-4d49-9950-05832ba291ae/manifest.json`; `91e9728f221d387f366eb9a9caa6337874349b6941a251b888bcd08ab6cf2f8a` |
| `pwsh -NoProfile -File scripts/test-p1.ps1 -Jobs 2` | Pass: 83 foundation/accounting/history/retained contracts | `artifacts/p1/035386d3-8281-420a-93d7-cb54e98328ae/manifest.json`; `54ee4c8199aadf398d3785ef3e138c3915e94bf66ae48847a9bd286abf00495d` |
| `pwsh -NoProfile -File scripts/test.ps1 -Suite fast` | Pass: delivery and source checks | `artifacts/tests/5822f9fe-85f2-4d53-a55e-f84374a9de80/manifest.json`; `074cd5cce88051dc7f44ca1201b107866b4ca9037bf77f36dab6636036387e89` |
| `pwsh -NoProfile -File scripts/test-context.ps1 -Jobs 2` | Pass: 23 native context contracts | `artifacts/context/8790b358-17c7-4e90-97ff-7f69b27b90e6/manifest.json`; `7eccb4cf811782f8e847b682426ec7373c93e0c36bd3c3359cc4cd67f04fd7ca` |
| Native `RecoveryTests` baseline command below | Pass: retained native recovery qualification | `artifacts/build/09d48f74-6cbd-4f90-9d06-4be309e83584/manifest.json`; `1294f44a3e1dfebad42144c82381911d643464ad2e185946ad56f3b22325aa72` |
| Native `LifecycleTests` baseline command below | Pass: all 29 retained regressions | `artifacts/build/9970e5fa-5801-4ebc-b685-6181aa59f6a6/manifest.json`; `be3e3e3dfcac19f0c21038524c526e104cc4787cb7539e639b65eb2ac74cdb5d` |

Recovery uses `pwsh -NoProfile -File scripts/upstream/build-baseline.ps1
-SelectedCodex -Mode RecoveryTests -ExperimentToolchain 1.98.0
-OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target -Jobs 2`.
Lifecycle uses the same command with `-Mode LifecycleTests`.
All commands above exited 0. After finalizing this report, the repository contract
checks and `git diff --check` also passed. Qualification source hashes are checked
against the staged Rust and PowerShell files before commit.

The selected upstream source and dependency lock remain unchanged. There is no
new third-party import, patch, model asset or provenance reconstruction to claim.

## Limits

This fixture proves native Node execution and canonical completion checks, not
arbitrary semantic coverage. Cargo parsing/discovery has contract evidence but
no real Cargo project execution fixture yet. Automatic model-tool verification,
end-of-turn completion, document-specific checks, independent-process verification
recovery, Git/index diffs, excluded dependencies and transient change/restore
between observations remain further work. Native file pins protect existing
selected bytes, not an atomic filesystem/store transaction over new entries.
The native CLI has no editor buffers; editor integration remains deferred.
The original P2 acceptance and later release gates remain intact.
