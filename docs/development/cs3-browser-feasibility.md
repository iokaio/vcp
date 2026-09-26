# CS-3 browser feasibility checkpoint

CS-3 browser execution remains unqualified. Bounded Windows experiments reached
browser startup diagnostics, but not accepted DOM/form interactions or an owned
server lifecycle. No `webapp-testing` package or default-skill promotion follows
from this checkpoint. The [CS-3 contract](../plan/24-skills-follow-on.md#cs-3--browser-execution-and-six-skill-acceptance)
and its six-skill acceptance gates remain unchanged.

No supported configuration-only route was identified for the installed browsers
that preserves both the zero-capability outer AppContainer and the browser's
internal sandbox. This is a bounded finding, not proof that every browser build
is incompatible. The owner decision about a newly provisioned isolated host or
different outer boundary is pending; no alternative has been selected or provisioned.

## Preserved execution boundary

Experiments used synthetic inputs and newly owned private staging/profile paths.
The browser received zero AppContainer capabilities and an explicit pipe-handle
allowlist. Assignment to the sole unnamed job was atomic at process creation;
kill-on-close and no-breakaway behavior remained enabled. Limits were 32 processes,
1 GiB per process and 2 GiB per job. Runtime files were read/execute; only owned
profile and scratch directories allowed modification. No signed-in user profile,
browser download, network capability, sandbox-disable flag or preference edit was used.

The controller retained process identities and an independent completion port.
The worker checked job membership, tokens, owner liveness and scratch bounds.
Startup-debug observations used a 20-second window, 512-event and 128-module
ceilings, with 90-second worker and 100-second controller deadlines. These were
diagnostic ceilings, not evidence that the corresponding acceptance gates passed.
Only the newly created broker was debugged; no attachment or memory dump was used.

## Retained attempts and actual outcomes

These portable run identifiers identify retained local evidence. A read-only
projection binds all nine exact receipt hashes, status, drainage and zero provider
calls; its SHA-256 is
`07c9ceb3801f99276198bcaa2ee9245ee5e8b0e537d01114dad4ca6f4fb8593c`.
Raw debug buffers, machine paths and generated receipts are not published.

| Run identifier | Observed result | Cleanup disposition |
|---|---|---|
| `df0c2b3ec5f540679c2afc3a3e83be4a` | Full Chromium preparation stopped at a profile-directory collision before launch | Cleanup succeeded |
| `a869e030c479486ebb746a670e08b0cc` | Full Chromium transport failure; primary-error capture was subsequently corrected | Cleanup succeeded |
| `d7b9a1b22d7e43a5926880633f273558` | Full Chromium Crashpad named-pipe access denial and exit `0x80000003` | Independent drainage and cleanup succeeded |
| `7a82d9128ce64327a3d77849b616a13b` | Controller failed during process-identity observation; no page/version result | Pending: independent drainage was not retained |
| `bfbf05329a3d4c3e9e6c64efb651e16e` | Headless CDP EOF; broker exit `0x80000003` before version | Independent drainage and cleanup succeeded |
| `e33dfb4f8a2941cd96c5815824efb15d` | Same startup failure; explicit stderr logging remained empty | Independent drainage and cleanup succeeded |
| `968f243581504af49645dbc9ef83ff49` | Chromium first/second-chance breakpoint at RVA `0x22A71B5` | Independent drainage and cleanup succeeded |
| `5564d07ad08a417a9f13d12d339148a8` | Firefox debug-string ceiling; original capture/notification defects identified | Pending: no independent ACTIVE_PROCESS_ZERO observation |
| `f1243a48e8e840868185e55762eb1534` | Corrected Firefox capture reached its real 16-KiB ceiling; child channels failed | Acknowledged independent drainage and cleanup succeeded |

The two pending profiles retain the original run identifiers and receipts. Worker
termination, a query reporting zero processes, or later successful cleanup code
does not retrospectively establish their drainage. Preserve them until separate
reconciliation evidence permits cleanup. The final Firefox profile was verified
absent after its successful cleanup; that does not resolve either earlier profile.

## Chromium: exact evidence and source interpretation

The installed headless-shell executable was 199090176 bytes. Its independently
computed SHA-256 matched the frozen distribution inventory:
`f7c1ef91a3e287b64509a9733fc8fd43678cf4e2765d67f4d0eceb0e39ecd026`.
The debug run retained first- and second-chance exceptions at RVA `0x22A71B5`,
followed by exit `0x80000003`, with no stderr or OutputDebugString evidence.

Static inspection found `int 3; ud2` in runtime-function range
`[0x22A6680, 0x22A71D2)`, which references `ContentMainRunnerImpl::Initialize`.
The incoming branch follows a nonzero result from vtable offset `0x10` with
argument 2, after checking the literal sandbox-disabling switch. No headless,
CDP or GPU-mode branch in this broker path avoids that call.

Current [Chromium sandbox initialization](https://raw.githubusercontent.com/chromium/chromium/main/sandbox/policy/sandbox.cc),
[broker interface](https://raw.githubusercontent.com/chromium/chromium/main/sandbox/win/src/sandbox.h)
and [desktop enum](https://raw.githubusercontent.com/chromium/chromium/main/sandbox/win/src/sandbox_policy.h)
map this shape to `CreateAlternateDesktop(kAlternateWinstation)` and its success
check. This is a strong source interpretation of exact binary evidence; it is
not exact-build symbolication. The underlying failing Win32 call/error is unknown.
The binary names `headless_shell.exe.pdb`, GUID
`871F6021-3AE0-9DF9-4C4C-44205044422E`, age 1; no PDB was acquired.

## Firefox: transport readiness is not child execution

Cached Firefox revision 1511 had 59 inventoried files totaling 332669198 bytes.
Installed AppConstants, application and platform metadata identify version
148.0.2 and BuildID `20260313222112`; AppConstants' source-revision URL is empty.
A matching Gecko source revision was not established from the installed metadata.

| Exact installed file | SHA-256 |
|---|---|
| `firefox.exe` | `1d6f3e064b5e8cbe90115e8f3c87b55f5cd244e56a3c2e468cacba68dd7d0cdf` |
| `xul.dll` | `a996016c322aabef0045ff89b1db12adfeafc3e8e816172fdff00e4bbb4de0f5` |
| `omni.ja` | `28f693c7d04a5f3ed1fabbbfa79afe072bd9b5d1ef73209ba0f09ab1ea5fe6a7` |

Installed Juggler sources and binary literals corroborated the documented
[inherited Windows pipe transport](https://raw.githubusercontent.com/microsoft/playwright/main/browser_patches/firefox/juggler/pipe/nsRemoteDebuggingPipe.cpp).
The corrected run emitted Juggler readiness, then repeated socket/tab subprocess
failures at `InitializeChannel(Error:0)`. Registry creation, I/O access-denied and
SWGL framebuffer errors also occurred. The broker remained alive until owner
termination; no second-chance exception or child-token observation beyond the
broker was retained. No protocol-driven page or form was accepted.

Exact `xul.dll` contains `InitializeChannel` and the UTF-16 pipe format
`\\.\pipe\gecko.%lu.%lu.%I64u` at file offset `0x90F6BA4` (RVA `0x90F83A4`).
Current [Gecko launch source](https://raw.githubusercontent.com/mozilla-firefox/firefox/main/ipc/glue/GeckoChildProcessHost.cpp)
initializes the child channel before process launch; its [Windows IPC source](https://raw.githubusercontent.com/mozilla-firefox/firefox/main/ipc/chromium/src/chrome/common/ipc_channel_win.cc)
hardcodes this format for pipe creation/opening. Microsoft's [named-pipe contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea)
requires the LOCAL namespace for the documented AppContainer case. This is strong
namespace-mismatch evidence, but the failing Win32 call was not captured.
`InitializeChannel(Error:0)` must not be relabeled as proven CreateNamedPipeW
access denied, or conflated with separate I/O error 5 messages.

## Diagnostic corrections and remaining decision

The first Firefox capture multiplied a documented byte length for Unicode and
retained data after its first NUL. A separate correction reads one complete code
unit at a time, stops at NUL and charges actual reads against unchanged caps.
Managed tests cover terminators, incomplete units, failed reads and exhaustion.
Old buffers and receipts remain unchanged and are not reproduced here.

The corrected worker holds the sole job handle after termination/query-zero until
the controller independently observes ACTIVE_PROCESS_ZERO and acknowledges it.
The final run validated this ordering and cleanup. Windows still documents these
[job notifications as advisory](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_associate_completion_port);
missing evidence remains failure. Neither correction establishes browser qualification.

A custom browser integration would require source work and new security evidence;
no configuration-only fix was found. A disposable Windows VM or explicitly
configured Windows Sandbox is an optional alternative outer boundary, not a selected
solution. Windows Sandbox was unavailable locally. Provisioning, network/sharing
restrictions, lifecycle acceptance and the boundary decision remain outstanding.
No sandbox-disable recommendation follows from these findings.

CS-1 owner acceptance retains its [additional qualification work](../../src/skills/candidates/README.md).
CS-2 specialist comparison/package qualification remains independently required.
Neither is waived by this prerequisite investigation; six-skill distribution,
activation/revocation, comparison and browser/server acceptance remain open.
