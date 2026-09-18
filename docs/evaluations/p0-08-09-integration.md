# P0-08/P0-09 — Retained integration and behavioral ports

Status: P0-08 and P0-09 complete for bounded native feasibility. Integration,
Gemini comparison, retained-core regressions and native recovery passed. This
is not production or release acceptance.

The [implementation guide](../development/p0-integration.md) specifies the
candidate interfaces and limitations. [The handoff map](../development/p0-handoff.md)
identifies retained modules, replacements and later task ownership.

## Integrated native trace

The final reviewed-source run of
`pwsh -NoProfile -File scripts/test-integration.ps1` passed with Rust 1.98.0
(`88d9e12ae`), MSVC 14.50.35717 and Windows 10.0.26200:
`artifacts/integration/d888f675-fa73-47c4-b6b3-4d27baaa4a6e/manifest.json`.
All stages exited zero: 27 VCP tests (five library, sixteen controller, three
integration and three neutral-port cases), two retained Windows regressions,
example build and a separate native coding CLI process. The CLI binary SHA-256
was `b98bf9a43ae61ca624f15c958cb43ec58c472a154c221e1edd3e82966b5305f7`.

An earlier version initially passed with
Rust 1.95.0 (`59807616e`), MSVC 14.50.35717 and Windows 10.0.26200. Evidence:
`artifacts/integration/8b578b63-77f7-44b9-a8c1-b9495e407d50/manifest.json`.
All stages exited zero: 25 VCP tests (four journal, sixteen controller, two
integration and three neutral-port cases), two retained Windows regressions,
example build and a separate native coding CLI process. The CLI binary SHA-256
was `3bb8de128b82b64cce092d6f849ef5418a80348f4e26a48d28a9e801eb280b44`.

The trace observed seven loopback HTTP Responses requests, each with the explicit
synthetic bearer token and only the allowed workspace tool. It read a real file,
parsed/prepared/applied a retained-format patch, launched the native verifier,
received exit 0 and the expected assertion text, proposed and recalled local
Munarium-governed memory, then received a summary containing the actual tool
result. The journal reopened with identical requests, effects and claims.
Seven settlements retained provider usage and charged 700 of an 800-unit cap.
These are declared flat synthetic units, not live provider prices or USD.

Negative cases preserved an intervening user edit, rejected unprepared or
rewritten patch arguments, rejected foreign-workspace
recall, retained a conflicting claim as disputed, rejected ungranted isolated
startup and an executable-tool bypass, and raced root/child reservations against
one remaining request allocation. Exactly one won; dropping its permit retained
unknown liability and prevented another dispatch. The observer saw zero provider
requests in that race. An actual interrupted provider stream produced exactly
one observed HTTP request, no hidden retry, and one unsettled liability that
survived reopen. Endpoint validation rejects ambiguous URL authorities, and
budget configuration rejects a volatile host. A legacy format-1 checkpoint
upgrades by appending format 2 without rewriting historical bytes.
Existing controller tests cover stream interruption,
pending receipts, stale revisions, native tree stop and deliberate readmission.

No live OpenRouter call was made. The offline profile proves the retained
Responses/host integration and explicit auth selection for those fixture paths;
it does not qualify every remote model, provider feature or credential flow.

## Gemini comparison

Command:

```powershell
node scripts/upstream/compare-gemini-ports.cjs --source artifacts/upstream/gemini-6a466a7e2fe2 --output-root artifacts/gemini-ports
```

The passing native Node 24.10.0 run is
`artifacts/gemini-ports/1ffcf37a-54fe-4ea1-a0c1-863eb7162729/manifest.json`.
It verifies immutable source revision
`6a466a7e2fe2b1255752c1e74f69b31f0216084d`, compiles the actual TypeScript and
records source/compiled hashes. Shared fixture SHA-256:
`79f263e060b121af7dfc1f1867496e2d7618219784a18894f66db085638c3854`.
The original 402-test baseline remains recorded in [P0-07](p0-07-gemini-baseline.md).

The first comparison failed because a provisional expectation omitted Gemini's
top-level null-byte structural separators. Inspection confirmed these were
meaningful policy boundaries. The Rust adaptation and neutral expectation were
corrected to preserve them; the original source expectation was not changed.

The comparison preserves call IDs, out-of-order completion and queued cancellation.
It also records the actual state-manager argument/outcome behavior and
`checkPolicy` client-origin shortcut. The Rust tests require fresh argument-bound
approval, trusted ceilings, durable admission and resource ownership until an
executing cancellation has an observed receipt. These are explicit VCP additions,
not claims about missing behavior in Gemini's higher-level scheduler. The
[component attribution](../../src/third_party/components/gemini-cli.md) records
source paths, Apache-2.0 terms, destination files and dependency closure.

## Maintenance and reconstruction

The representative fix is OpenAI Codex commit
`9daa491f7c27a5513fec554473a7122d88fca367`, “Harden local MCP server process
tree cleanup”, parent `81b9bc210926b14b2af5c3300f13972909266aab`.
It is already included in VCP's selected upstream revision. This is an explicit
retrospective import experiment, not a dependency upgrade.

`scripts/upstream/rehearse-codex-fix.cjs` passed in
`artifacts/codex-fix-rehearsal/74cd3b7c-2fd8-4bf1-8a5e-51d66126f566/manifest.json`.
The original fix changes six files, adding 440 lines and deleting 55. The VCP
Job Object observation overlay adds 24 lines. The rehearsal first exposed an
import-context conflict: that overlay referenced imports introduced by the fix.
Reconstructing the six parent files, applying the immutable upstream fix, then
reapplying the unchanged overlay resolved it. All six results matched the
expected bytes, including exact equality with VCP's maintained Job Object module.

Three local attempts were recorded: initial ordering conflict, a Git CRLF
materialization mismatch, then success after explicit `core.autocrlf=false` in
the disposable repository. No global Git setting, upstream pin or unrelated
source changed. These observations measure a bounded maintenance exercise, not
forecasted release maintenance effort. The retained
`contained_spawn_owns_immediate_descendant` and
`rejected_job_assignment_resumes_existing_job_member` tests each passed natively.
VCP's controller suite independently passed its descendant-stop/lock observations.

Full ordered reconstruction passed for all 7,938 selected Codex files:
`artifacts/reconstruct-p0-integration.json`, resulting digest
`8a9dcc2f17aaff06967a22a03df03f972ac611c148be82f779f6489b75937e5d`.
Patch `0010-integrated-host.patch` SHA-256 is
`43a6f4986468880e10b131179cd4e359aac09abf58a526d96980956321548b19`.
The patch adds host/test seams and existing local dependencies, without changing
external package identities. Ordinary builds still consume committed source.

## Retained native regressions

The common Rust 1.98.0 candidate also passed the retained lifecycle gate:

```powershell
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode LifecycleTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
```

The lifecycle run exited zero with 29 tests, including retained core
review/mailbox/continuation and VCP controller cases:
`artifacts/build/26518fd7-2ce9-444f-8aa7-b1f982cb6e31/manifest.json`.
The recovery run exited zero with four journal tests, sixteen controller tests,
the independent owner-process pause/reopen observer and native execution/access
canaries:
`artifacts/build/3d1a167e-cbc3-4ff1-92a7-7910e9d6f606/manifest.json`.
The descendant-stop observer recorded two job members before termination and
zero afterward, with the exclusive lock released in 17 ms. This is one native
observation, not a latency guarantee.

The first recovery invocation under the development tool's restricted sandbox
failed with access denied before compilation. The same command passed with
access to the installed toolchains and native Windows process controls. No test
or product boundary was disabled. Source-hash checks confirmed that all 18
integration inputs and all 14 lifecycle source inputs still match their passing
manifests after the editor restart; the independent reconstruction record also
matches the complete current Codex inventory byte for byte.

## Acceptance scope

R02–R05 prototype evidence consists of prepared-file stale rejection, host tool
ceilings, native process ownership, neutral argument/confirmation/resource cases,
and request reservations/receipts through the retained coding loop. R08 evidence
is the selected-source/rights record, ordered reconstruction and fix-import
rehearsal with native regressions. Complete production and release R matrices
remain with their owning P1–P8 tasks. No public API/editor/hook requirement was
silently promoted into this P0 scope or declared shipped.
