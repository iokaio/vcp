# Internal beta manual-testing checklist

Current handoff: [October 1 candidate, artifacts and observed results](05-manual-candidate.md).

This milestone supplies one exact unsigned Windows installer/ZIP/VSIX pair for
owner-led testing. It does not close the [release qualification matrix](00-release-plan.md#5-required-beta-qualification-matrix),
establish clean-host qualification, or authorize public publication. Record each
observation as **pass**, **fail**, or **not run** against the candidate's pair ID.
Use the [implementation ledger](02-implementation-status.md) for existing evidence
and unresolved findings; an earlier failed pipeline remains a failed pipeline.

## Before handing over the candidate

- [ ] Identify the reviewed source, pair ID, native/VSIX versions and SHA-256 of
  the unsigned setup EXE, ZIP and VSIX. Verify the artifacts against the pair's
  receipts and checksum record; include its packaged notices and user guides.
- [ ] Record the focused source, packaging and installer checks selected for
  this increment, their actual results and known failures. Reused evidence must
  identify its original bytes and explain why it applies.
- [ ] Record a basic smoke of these bytes: per-user install, launcher version and
  resolved engine/data identity, VSIX installation/connection and setup-guide
  access, then removal with test data preserved. Label the actual environment.
  A developer-host smoke is **not** clean-host qualification or completion of the
  full installed matrix. Preserve any failed installation for diagnosis.
  An offline observer connection may use explicitly identified synthetic retained
  history; it does not prove a first useful live task.
- [ ] Supply artifact locations, the remaining limitations and a reporting route.
  Do not hand off mismatched artifacts or a known permission, secret-disclosure,
  data-loss or uncontrolled-process defect as an acceptable beta limitation.

## Owner walkthrough

Use a nonsensitive sample workspace. Keep data and profiles in separate private
directories outside repositories and synchronization roots. Keep an independent copy of any
sample files you intend to edit. Record the pair ID, Windows build, VS Code
version and chosen program/data/workspace roots privately.

1. **Install and identify.** Follow [installation](../usage/beta-installation.md)
   on native Windows x64 with local NTFS paths, PowerShell 7 at its supported
   Program Files location, and VS Code **1.138.0**. Follow organizational policy
   for unsigned software. Run the documented `--version` and
   `--resolve-installation` commands through the stable installed launcher.
   Confirm the selected data directory; setup does not add VCP to PATH.
2. **Prepare the first task deliberately.** Follow
   [onboarding](../usage/beta-onboarding.md) using a sample `README.md`.
   Provider setup and task execution wait until the owner supplies credentials
   and explicit spending caps; mark these observations **not run** otherwise.
   Setup probes and tasks have separate caps. Use the masked credential entry,
   complete `setup provider`, `setup profile` and `setup check`, then run the
   documented cited read task. Check its result, recorded cost and unchanged
   source files. Do not retry unresolved provider liability as a fresh task.
3. **Connect the installed extension.** Follow the
   [VSIX setup guide](../../src/packages/vscode/SETUP.md), available through
   **VCP: Open Setup Guide**. Set `vcp.engineExecutable` to the resolved
   **versioned native executable** and `vcp.dataDirectory` to the resolved data
   directory in **User** settings. Use the workspace initialized by the first
   native task. Connect as an observer and check workspace identity, retained
   task state and cost. Reload/reconnect should not resume work or acquire control.
4. **Explore a small useful task.** If desired, use
   **VCP: Start Execution-backed Task** with the matching trusted profile and
   owner-approved limits. Review VS Code and engine trust separately. Inspect
   questions, pause/resume and retained outcomes. For an edit, follow the
   [documented draft/review/apply flow](../../src/packages/vscode/README.md):
   approve deliberately, inspect the actual buffer diff and save explicitly.
   The generated profile has no external test runner; do not claim project tests
   ran unless you configured and observed them.
5. **Remove without erasing work.** Disconnect clients and close engine owners,
   allowing their normal shutdown. Follow the
   [uninstall instructions](../usage/beta-recovery.md#uninstall). Check that the
   owned program/registration are removed while sample files, selected data,
   profiles and independent recovery material remain. If removal refuses, retain
   the diagnosis and roots; do not delete ownership markers or bypass locks.

Portable ZIP use is an optional separate observation: follow the
[portable instructions](../usage/beta-installation.md#portable-use-and-optional-local-models),
use a fresh extraction root and invoke its native `vcp.exe`. Optional models and
skill dependencies are acquired only when the owner chooses that workflow.
Distinct-version upgrade/rollback, independent-machine recovery, network-denied
operation and the rest of the matrix remain separately recorded work; this
checklist makes no support claim for them.

## Capture useful feedback

For each attempted step, record expected versus actual behavior, candidate ID,
environment, pass/fail/not-run and a short sanitized error or observation. Keep
private originals locally. Follow [safe reporting](../usage/beta-known-issues.md#report-a-problem-safely):
do not upload credentials, environment dumps, raw provider/setup logs, profiles,
task histories, repositories or recovery keys. Remove the shell credential after
testing using the onboarding guide's cleanup command. A useful manual-testing
handoff and completed release acceptance remain different outcomes.
