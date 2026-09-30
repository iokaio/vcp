# Internal beta: upgrade, removal and recovery

Use the exact compatible artifact pair identified in the beta scorecard. Current
formats are `vcp-store/1+replay-base/2` and `vcp-cli-profile/1`; product version
numbers do not imply a state migration. Arbitrary downgrade and cross-format
migration are unsupported. A candidate's real upgrade/rollback observations must
name two distinct builds and retained state; synthetic script tests are separate.

## Upgrade and rollback

Pause work deliberately and let the engine finish its retained drain. Close the
CLI and disconnect editor clients before changing the installation. Reopening a
workspace observes retained tasks; it does not resume them. Keep independent
recovery material in its existing private location.

Run the newer approved setup at the same program root. Setup preserves the owned
data root and retains the previous engine release. Concurrent setup, uninstall or
engine lifecycle operations are refused while their respective operation lock is
held. Unsupported retained format markers, active store owners and redirected
paths refuse activation. Resolve the reported problem before retrying.
For a retained SQLite WAL, recover and close the workspace with its existing
engine first. Setup does not checkpoint or delete the WAL.

For a recorded compatible previous release, use the packaged engine maintenance
script. Substitute the actual roots; the engine root is inside the program root:

```powershell
$program = Join-Path $env:LOCALAPPDATA 'Programs\VCP'
$data = Join-Path $env:LOCALAPPDATA 'VCP'
& (Join-Path $program 'maintenance\package-install.ps1') -Action Rollback `
  -InstallRoot (Join-Path $program 'engine') -DataRoot $data
if ($LASTEXITCODE -ne 0) { throw 'Rollback failed; preserve the installation and inspect its diagnosis.' }
& (Join-Path $program 'vcp.exe') --version
& (Join-Path $program 'vcp.exe') --resolve-installation
```

Install the matching older VSIX only if that exact downgrade is approved by the
scorecard. Refresh explicit User engine/data settings and reconnect as an observer.
Inspect the task, revision and unresolved accounting before explicitly resuming.
Never retry an unknown effect by creating a second task to conceal its outcome.

## Interrupted installation

The engine activates through a validated pointer after extraction and verification.
If setup fails or is terminated, retain its log and run the same candidate again.
An abandoned operation lock does not prove the previous operation completed.
Owned validated releases can be recovered; unexpected files or unfinished staging
may require investigation. Do not delete history, ownership markers or staging
contents to force success. A locked or tampered payload can cause uninstall to
refuse while preserving its registration for a later repair attempt.

## Uninstall

Use Windows **Installed apps → VCP Internal Beta → Uninstall** as the installing
user. Close engine processes first. Removal preserves the selected data directory,
workspaces, separately provisioned models/profiles, encryption keys and synchronized
vault. It removes owned program files, registration and selected shortcuts. If it
reports unexpected or locked files, resolve them and retry; do not recursively
delete the program root as a substitute for the ownership checks.

Uninstall is not a command to erase history. Use VCP's explicit retention/purge
workflow when you intend to remove retained records. Keep the independent keys
needed to open encrypted snapshots.

## State and encrypted recovery

Local histories and profiles are plaintext and belong outside sync roots. Publish
only through the configured encrypted snapshot workflow. Preserve recovery keys
independently from the vault; do not place either in a support report. Importing a
snapshot does not authorize provider work or resume a task.

Use the CLI's `backup --help`, `restore --help` and `storage --help` to inspect the
supported commands for the installed version, together with the paired release's
recovery evidence. Do not replace individual canonical database/journal files or
rewrite format markers. Missing keys, tampered ciphertext and incompatible state
must refuse safely. Independent-machine recovery and physical full-volume behavior
remain separately recorded gates; a same-host test does not establish either.

For a problem report, follow [safe support instructions](beta-known-issues.md).
