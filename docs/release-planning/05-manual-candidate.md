# October 1 manual-testing candidate

The limited BETA-08/BETA-09 handoff under [ADR-073](../adr/073-manual-testing-candidate.md)
is ready for owner manual testing. Full qualification and BETA-11 acceptance remain
open. Start with the [manual checklist](04-manual-testing-checklist.md).

## Obtain this exact pair

Download the [candidate packet](https://github.com/iokaio/vcp/actions/runs/36868151228/artifacts/11167895010)
from [successful attempt 2](https://github.com/iokaio/vcp/actions/runs/36868151228/attempts/2).
The workflow retains it for 90 days. Its `artifacts/` directory contains the setup
EXE, portable ZIP and VSIX; `receipts/`, `pair.json`, `evidence.json` and `SHA256SUMS`
identify their source and actual qualification status.

- Reviewed source: `f81b2c5062dad1f8d13d72ffb5fb8006eeea9229` (merged [PR #324](https://github.com/iokaio/vcp/pull/324)).
- Pair ID: `1dba45922e0c34a04880f2ed4c5a07a65882b809559104aee70506b88838bcc7`.
- Native version: `0.2.0-beta.1`; SDK/VSIX: `0.2.1`. All artifacts are unsigned.
- `SHA256SUMS` SHA-256: `661d35101282872aa7680aa364448c1f96429484438bb37a61c858e10db5ef46`.

| Artifact | SHA-256 |
| --- | --- |
| `vcp-0.2.0-beta.1-windows-x64-unsigned-setup.exe` | `7d6fdc48bc6b273e30b3738dd45664d80484eebe2bb94d6169a09c37547b44ae` |
| `vcp-0.2.0-beta.1-windows-x64-unsigned.zip` | `915c0d36428709e01c5a0e13f393a15fbbdd0986c93624646ddeef408ddca53d` |
| `vcp-local-0.2.1-win32-x64.vsix` | `d7d58e969bf6857b607d9e70e65349dd32645f90c1434292d10627073ce4e10b` |

Use Windows x64, local NTFS directories, PowerShell 7 at its standard Program
Files location and VS Code **1.138.0**. Follow [installation](../usage/beta-installation.md),
[onboarding](../usage/beta-onboarding.md) and [known issues](../usage/beta-known-issues.md).
The ZIP includes the user guides, prerequisites, component inventory and notices.
The selected bytes predate this handoff record; this documentation does not
change their source identity or require a rebuild.

## Observed results

[Main Delivery checks](https://github.com/iokaio/vcp/actions/runs/36867555060)
passed. The candidate used `vcpwin` in `wingroup`, 16 build jobs, Windows build
image `win25-vs2026` (`10.0.26100`). Portable checks and every selected stage
through `pair` passed. Production compilation used the release profile without
qualification features; source, dependency and tool identities remained stable.
The production-build stage took 15 minutes 28 seconds.

Independent local verification checked all **88** retained checksum entries,
recomputed the pair from the unchanged receipts and archives, and verified the
extracted native inventory. Focused smoke used these exact installed bytes on a
developer workstation running **Windows 10.0.26300**, with pinned VS Code 1.138.0:

| Observation | Result and limit |
| --- | --- |
| Registered install, launcher/version and engine/data identity | Pass. Custom Unicode program/data paths; actual per-user registration. |
| Files and SQLite storage preference, uninstall preservation | Pass. Both real preferences and the synthetic data sentinel retained unchanged. No canonical task created by this check. |
| Offline onboarding guidance | Pass. Three setup help commands and two missing-input cases produced expected guidance and exit codes. No provider calls. |
| Installed VSIX connection, both stores | Pass. Observer saw the retained paused synthetic task on each store; no development extension path used. |
| Packaged setup guide | Pass for reading the installed guide and invoking `VCP: Open Setup Guide`. Visual usability remains not run. |
| Editor removal and retained files, both stores | Pass. Native/VSIX removal completed; fixture file paths and hashes unchanged except the canonical owner lock's expected PID/nonce rewrite. No new SQLite sidecars remained. Workspace directories were empty, not user-edit preservation fixtures. |
| Process ownership | All four successful local runs exited naturally, without timeout or forced cleanup; their owned process trees were empty. |

The smoke reused existing release scripts. A disposable driver copy added only
packaged-guide access; the production VSIX was unchanged. Private local evidence
is indexed by `artifacts/beta-delivery/manual-candidate-36868151228-smoke.json`
(SHA-256 `04d6bdc6f714b4d928d72b950077dedbd3919b76d1a97d4e9b99df84e113bb7f`).
It references the actual result/supervision hashes. Private roots and raw logs
are retained locally, not published with this document.

## Limits and retained failures

The packet correctly says `selection_status=pass`, `pipeline_status=incomplete`
and `status=qualification-required`. Its later native-boundaries, installed-native
and installed-editor qualification stages remain **not run**. The focused local
observations above do not relabel those stages or close the full matrix.

Attempt 1 failed before compilation with HTTP 504 during tool provisioning; its
failed packet is retained. The first local editor invocation rejected a
mixed-separator editor path before installation. Correcting the one-off invocation
allowed the same candidate bytes to pass; that failure is also retained.
Earlier candidate/native-preflight failures and the deferred 7-pass/1-fail process
test remain recorded in the [implementation ledger](02-implementation-status.md).

No live provider setup, first useful task, reviewed edit, clean-host campaign,
final console-cancellation campaign, distinct-build upgrade/rollback or full
qualification matrix was run for this handoff. The owner walkthrough requires
credentials and explicit spending caps before paid execution. No public release,
Marketplace publication or wider distribution is authorized by this record.

Implementation stops at this handoff. Record manual feedback using the checklist
and [safe reporting instructions](../usage/beta-known-issues.md#report-a-problem-safely);
resume engineering for concrete findings or separately selected qualification work.
