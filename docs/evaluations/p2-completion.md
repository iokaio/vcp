# P2 reliable local coding qualification

Status: P2-01 through P2-08 complete on September 19, 2026 for the internal
Windows coding baseline in plan segments 04–06.
The implementation includes the remaining retry, scheduling, recovery and
handoff work identified during the completion audit. This is not an installed
CLI, general OS sandbox, memory, grouped routing, skills/MCP or delegation claim.

The implementation is committed through `c0e3dc0` on the completion branch,
building on main `dc0fec52ed4eac31c363f56881e53f15015c215a`. The source-hashed native manifests
below identify the tested inputs, including uncommitted inputs at qualification
time. The [traceability ledger](../plan/20-traceability.md) and
[implementation guide](../development/p2-completion.md) describe the resulting
boundary; earlier increment reports remain historical evidence.

## Acceptance map

| Task | Qualified boundary |
|---|---|
| P2-01 | Bounded Git/worktree discovery, no-follow roots, generated/binary/oversize exclusions, stable file versions, parent/nested AGENTS.md applicability, captured selection, envelope accounting and pre-send invalidation |
| P2-02 | Normalized requests/streams/usage/errors; actual retained controller retries with fresh reservations, predecessor links, bounded backoff and one absolute deadline; raw-only HTTP error capture; uncertain liability and pre-send release; dated catalog and capped two-model live compatibility |
| P2-03 | Plan/ask/workspace/autonomous policy, trusted denial precedence, operation-bound grants/questions, headless waiting, authority-change fencing and native file/process enforcement |
| P2-04 | Prepared/versioned file operations, partial per-file receipts, path/link/version rejection, explicit direct/shell/PTY profiles, argv/environment isolation, output/deadline/process bounds and Job Object tree ownership |
| P2-05 | Retained admitted coding loop; complete-response tool eligibility; trusted resource scheduling, shared reads, conflicting-write serialization, bounded queues/concurrency and out-of-order correlation; scoped-generation fencing; empty-response failure; explicit memory/routing capability states |
| P2-06 | Actual project/Cargo/documentation checks, passed/failed/not-run/stale distinctions, source/instruction applicability, unresolved-effect and final-cost fences on both stores |
| P2-07 | Durable intent/outcome ordering, bounded cancellation, parent/child holds, real native console close versus forced termination, current file/process-receipt reconciliation without replay, quiescence checks, PID-reuse safety, exclusive reopen and fresh environment/budget validation |
| P2-08 | Complete-pair compaction, current-source validation, preserved corrections/effects/costs and originals; durable handoff packets with scope/current state/base/diff/remaining budget, referenced artifacts and explicit opaque-field omissions; compatible destination reassembly and fresh-process continuation |

## Native commands and results

Qualification uses Windows `10.0.26200`, NTFS, MSVC `14.50.35717`,
Node `24.10.0` and Rust `1.98.0` (`88d9e12ae`). Each native runner
verifies the retained source selection and rejects changes to its hashed inputs
during testing. Test counts overlap across focused gates; they are not a count
of unique tests.

- `pwsh scripts/test-context.ps1 -Jobs 4`: **pass**, 30 contracts.
  Manifest `artifacts/context/41e98dd7-6951-499d-b73a-9a2919412a50/manifest.json`;
  SHA-256 `a47f4a261561b0c8b254d742e1b66940765598efed898ca8ca620a11eda54ef9`.
- `pwsh scripts/test-provider.ps1 -Jobs 4`: **pass**, 25 contracts and
  the retained absolute-deadline regression.
  Manifest `artifacts/provider/b3d19dc1-f30e-4c5b-9e34-8f0a54cfeb91/manifest.json`;
  SHA-256 `10c1f4c44c7c8381c562142eeff28c32fbb0aaf91d95d97f837d8bc911679f26`.
- `pwsh scripts/test-policy.ps1 -Jobs 4`: **pass**, 47 contracts.
  Manifest `artifacts/policy/214ba74b-ac79-4a9e-806e-356a1443408f/manifest.json`;
  SHA-256 `26e821b2dc85acbd573a9d7199f3bcb81d918e30040936fa16805d2f974df027`.
- `pwsh scripts/test-tools.ps1 -Jobs 4`: **pass**, 31 contracts and
  retained patch regressions.
  Manifest `artifacts/tools/7566646b-99cc-425f-9908-b6943c3daf6f/manifest.json`;
  SHA-256 `bf1deec07387592d699b1b18ca1a40ab3bcefd55cb9c0c19f6c4c5bf676b521c`.
- `pwsh scripts/test-integration.ps1 -Jobs 4 -Toolchain 1.98.0`:
  **pass**, all 60 lifecycle/canonical/port/console contracts, the expired-header
  deadline regression, two native PTY containment regressions, example/binary
  builds and an independently observed seven-request private owner CLI trace.
  Manifest `artifacts/integration/3b5becf8-dc8c-44f9-8614-27abb01cb91d/manifest.json`;
  SHA-256 `679f5976a3c4fa926724a7d82381c0d5cee930c48fafddc6f831a6f8e73d7bf7`.

Repository delivery checks (`pwsh scripts/test.ps1 -Suite fast`) passed all eight
groups. Rust formatting, `git diff --check` and retained Git byte/mode validation
also passed. No dependency was added or upgraded.

The retained Codex change is reproducible patch `0021-p2-controller-retries.patch`,
SHA-256 `2a7a3906496473cf603411c5a4f646796e4e6bf8600f7eabf22951982874322c`.
Independent reconstruction matches the checked-in selection and full inventory
exactly (`files_sha256`:
`c19144683c93086651a0ebe1d3029c37d8028a672be18b21cf53e7354297b9fc`).
The record is `artifacts/p2-final-reconstructed.json`.
[Retry-specific evidence](p2-provider-retries.md) describes the scripted
transport matrix and exact-deadline assertion.

The separately authorized [OpenRouter live smoke](p2-openrouter-live.md) passed
on September 19, 2026: two fixed Responses API requests with requested/served
identity, usage and cost under the configured cap. That dated compatibility
observation was not repeated or misrepresented as live retry qualification.

## Qualification corrections

The first integrated run exposed an immediate SQLite reopen race. Backend
shutdown now waits for its native worker before releasing store ownership, and
the public close/reopen regression does not sleep or retry. The next run exposed
two coding-path errors: valid empty responses incorrectly fenced inspection,
and handoff setup initialized accounting before context denial. Empty answers
now retain capture/settlement and pause without permitting completion; denied
context still creates no ledger. The original denial assertion is retained.

The failed manifests remain at
`artifacts/integration/b35bfff8-1ccb-481d-8629-e4abc19262a5/manifest.json` and
`artifacts/integration/c28b0951-f8d9-4ac5-84e8-f98e42bb598d/manifest.json`.
Additional review regressions cover expired queued work, native startup-evidence
failure before resource release, and expired HTTP deadlines before transport.
The final green integration manifest above includes these corrections and rejects
any change to its recorded source inputs during qualification.

## Boundaries retained by later phases

P2 supplies an internal coding host and private owner-control fixture. P3 owns
the installed structured/interactive CLI, inspectors and workspace chooser.
P5 owns governed memory/retention; P6 owns grouped routing and model switching;
P7 owns skills/MCP/delegation; P8 owns general Windows sandbox and release
qualification. Portable P2 handoff validation does not itself authorize a
dispatch or implement the P6 routing policy.

Native terminal profiles accept bounded initial input bound to the prepared
operation. Ungoverned follow-up input/resizing is not exposed. Reduced isolation
is explicit; unsupported required filesystem/network restrictions are rejected.
Console owners must install and retain the canonical close-handler guard.
Forced termination cannot depend on that guard and is independently tested.

Recovery classifies observed state, not causal authorship. Missing or
contradictory receipts retain uncertainty; neither an absent success record nor
a reused PID permits replay or signalling. Hardware power-loss durability,
arbitrary external-effect observability and editor dirty-buffer guarantees are
outside this phase. Synthetic provider fixtures prove control flow and
accounting, not model quality; catalog/pricing observations remain dated.
