# P2 prepared process increment

P2-03 and P2-04 remain `in_progress`. The original process broker shares
canonical authority, questions and grants with file tools and wraps the retained
Windows Job Object launcher. See the [source guide](../development/p2-process.md).
PTY and retained-loop integration, in-flight steering/revocation coordination,
and full restart reconciliation remain required work.

## Acceptance scope

| Boundary | Native checks |
|---|---|
| Preparation | Explicit executable profile, source snapshot, directory identity, public environment, time/output bounds and schema; model authority fields and colliding root IDs reject; executable read denials apply before filesystem access |
| Pinned inputs | Executable identity/bytes and declared read-only scripts remain held; stale sources and profile replacement reject before launch |
| Policy | Restricted capabilities reject without dispatch; plan mode denies; exact approved invocations can be reused after deliberate resume; automatic fixture authority explicitly covers executable roots |
| Arguments | Direct Unicode/spaced/metacharacter argv is preserved; cmd and PowerShell execute explicit scripts with quoted redirection and Unicode paths |
| Output | Concurrent captures, combined ceiling, bounded tails, durable partial artifacts and explicit stop reason |
| Deadline | Real child/grandchild termination; kernel membership and independent exclusive-file-lock release are observed |
| Owner close | Bounded observers finish lifecycle receipts before checkpoint closure; canonical results remain available while paused |
| Conflicts | A file operation cannot dispatch while the process owns the workspace conflict lease |
| Stores | Native process trace executes against SQLite and files/journal, with no provider requests |

## Defects found during qualification

The directory-only test showed that attribute-only handles did not exclude
rename. Guards now request directory read access; pinning and creation in an
otherwise empty held directory have native regressions. Earlier file tests also
held child-file handles and did not isolate this case.

Owner close initially raced the caller's delayed process receipt. Bounded
processes now have an owned observer independent of caller polling; shutdown
drains it before checkpoint closure. Existing unbounded P0/P1 receipt behavior
remains in the retained regression surface.

Ordinary argv escaping corrupted cmd scripts with quoted redirection. Explicit
cmd profiles now use raw command-line conversion and the outer quotes expected
by `/s /c`; direct executables retain ordinary argv.

An initial file-broker run reported an incomplete result. The test now prints
the complete result on failure and passed after the directory-guard changes.
The original log did not identify the operation's cause; this report does not
attribute that result to a specific defect.

A subsequent full integration run timed out waiting for a canonical policy
command after 30 seconds. A synchronous worker self-reentry guard now fails
closed immediately if that condition occurs; focused and full retries passed
without triggering it. The original timeout's cause is unresolved, so those
passes do not establish that self-reentry caused it. The deadline remains intact.

## Reproduction and source identity

The native environment is Windows 10.0.26200, NTFS, MSVC 14.50.35717 and the
explicit Rust 1.98.0 experiment. The selected upstream pin is unchanged.
The [guide](../development/p2-process.md#local-checks) identifies the required
tool, integration, recovery, lifecycle and fast commands.

The final process source passed these local commands (all exit 0):

| Command | Outcome | Ignored manifest |
|---|---|---|
| `pwsh -NoProfile -File scripts/test-tools.ps1` | 28 contracts and 65 retained patch tests | `artifacts/tools/380638c0-177c-492f-a868-06117c29f70e/manifest.json` |
| `pwsh -NoProfile -File scripts/test-integration.ps1` | 35 native host/port/canonical contracts, two retained launch regressions and private CLI checks | `artifacts/integration/bfbaec9a-8563-4e3d-bed2-1fbf3a8ee9ed/manifest.json` |
| `pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target` | Native independent-process recovery qualification | `artifacts/build/ab431f15-8219-404e-9617-1162c7f73fac/manifest.json` |
| `pwsh -NoProfile -File scripts/test.ps1 -Suite fast` | Eight delivery/source cases | `artifacts/tests/baf164b5-b7ee-4f93-927b-09d4026c2aff/manifest.json` |
| `pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode LifecycleTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target` | 29 retained lifecycle regressions | `artifacts/build/733f222b-88e4-4cfe-ae20-51aa2237adaa/manifest.json` |

Manifest SHA-256 values, in the same order:

- `9dbbf2b36f6f30e864b0ca5ec5e3bee8279b8d77d15a333f505767e41c4ce796`
- `e2c1487ee9c8158b394941d945ead8057a3af85b8a7259d05f0bea3b162e4945`
- `0bbd399ea60f4f133ce5856e15159d28d4700a65eabb0a5d5311cde7f47486b1`
- `6d7a2cb39b516f2fec4eaddc68b8f511363310eab23f2ee504ebe75d3137593d`
- `5db68c9c9da58d272e5b9174be60167d5d9d4b61914ef65a8c6e896efec59e73`

The first fast invocation could not execute its source-hashing subprocess under
the agent sandbox; the normal-access rerun passed. Upstream index verification,
boundary inventory and repository link/task checks also passed. Local checks do
not substitute for the PR's remote CI result.

New product code and fixtures are original VCP source. The Codex tree and
external dependency pins are unchanged: 7,938 selected files, aggregate
`f08100ac0284dc4cc1dc2273037b0178181729caa483ceadfbe1c3bbe62fd6ec`.
The inventory has 172 packages, 35 groups and 81 named seams.

## Limits

Profiles provide process ownership, explicit environment, deadline and output
controls, without a general filesystem/network sandbox. Unavailable isolation
is denied; reduced isolation must be explicitly configured. Dynamic dependencies
outside declared pinned inputs remain opaque effects. Failed/cancelled programs
can leave partial effects; PIDs and missing success receipts never authorize replay.

This is an internal broker increment, not completed P2 or an installed product.
No paid provider call or hosted heavy Windows run is included.
