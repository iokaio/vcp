# Machine B operator command

Use a **second Windows installation**, PowerShell 7.4 or newer, and an installed
Git for Windows. No Rust, Visual Studio, model assets or provider credentials
are required. This step does not run model work.

Receive the ordinary package and its trusted manifest independently of OneDrive
(for example, on USB). Keep these files together:

```text
VCP-U04/
  package.json
  vcp.exe
  p3-portability-machine-b.ps1
  p3-portability-handoff.ps1
```

Receive the recovery directory separately. Preserve its relative subdirectories
exactly; do not place recovery files or the package in OneDrive. The script
passes recovery **paths** to VCP and never emits key contents.

Wait for the dedicated `VCP-U04/<campaign>/...` vault objects to appear through
the actual OneDrive client on B. Keep OneDrive running. Then run:

```powershell
pwsh -NoProfile -File E:\VCP-U04\p3-portability-machine-b.ps1 `
  -PackageRoot E:\VCP-U04 -RecoveryRoot F:\VCP-Recovery `
  -OperatorConfirmsActualOneDrive
```

If more than one OneDrive root is configured, add `-OneDriveRoot 'C:\...\OneDrive'`.
An alternate installed Git can be selected with `-Git 'C:\Program Files\Git\cmd\git.exe'`.
Private data defaults to `%LOCALAPPDATA%\VCP\U04\<campaign>\B`; `-LocalRoot`
can select another new, nonsynchronized directory.

The script validates the executable and incoming ciphertext against the supplied
manifest, restores both backend directions, checks source bytes and stable IDs,
and checks the supplied accounting, child and claim baseline. It verifies exact
restore retry, creates fresh synthetic Git metadata without restored hooks,
explicitly rebinds and trusts maintenance access, then publishes B descendants
to the same real vault. It does not silently remove existing policy denials.

After success, wait for OneDrive to report upload complete. Return the printed
`machine-b-receipt.json` file to A. Do **not** send the private step journals,
stdout files, data directory, staging directory or recovery files as ordinary
evidence. Keep them locally until qualification and cleanup are complete.

A completed rerun with the identical package returns the existing receipt without
publishing again. A partial run is retained and refused: inspect the recorded
step and operation before reconciling it. No recursive deletion or automatic
replacement of a partially restored workspace occurs.

`-LocalSimulation` is reserved for local script tests. It is mutually exclusive
with the real OneDrive confirmation and records that no cloud qualification was
performed. The B receipt alone never claims full U04 completion: A must observe
the actual return transfer and validate the descendant while preserving its prior
active root.
