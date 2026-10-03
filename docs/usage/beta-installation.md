# Install the internal beta candidate

These instructions target Windows x64 native, installer and SDK/VSIX `0.2.3`.
This candidate remains an internal beta and requires Ioka LLC code signatures.
Use the matching candidate handoff; a source version bump does not publish it. Check
the accompanying scorecard before manual testing: building an installer does not
mean its installed-product gates passed. Do not distribute it until the recorded
owner decision authorizes distribution. See [known issues](beta-known-issues.md).

## Prerequisites and artifact identity

Use native Windows x64 and local NTFS paths. The candidate's scorecard names the
actual Windows build tested; no general Windows version range or minimum hardware
claim is made. VS Code integration targets exactly **1.138.0** on Windows x64.
Remote, WSL, virtual and container workspaces are excluded.

Setup requires PowerShell 7 at `C:\Program Files\PowerShell\7\pwsh.exe` (or the
corresponding system Program Files directory). Obtain it through your approved
software process before setup. Setup diagnoses its absence and downloads nothing.
Installing the CLI does not require Node, Rust or MSVC. Optional skill helpers
have separate prerequisites in the supplied `PREREQUISITES.md`.

Obtain the setup EXE, portable ZIP, VSIX and release-pair/checksum records together
from the approved internal handoff. Compare each local SHA-256 to that record:

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath '.\vcp-0.2.3-windows-x64-signed-setup.exe'
Get-FileHash -Algorithm SHA256 -LiteralPath '.\vcp-0.2.3-windows-x64-signed.zip'
Get-FileHash -Algorithm SHA256 -LiteralPath '.\vcp-0.2.3-win32-x64.vsix'
Get-AuthenticodeSignature -LiteralPath '.\vcp-0.2.3-windows-x64-signed-setup.exe' |
  Select-Object Status, StatusMessage, SignerCertificate, TimeStamperCertificate
```

The signed candidate must report a valid signature from **Ioka LLC**, with a
timestamp, on its setup, native engine, stable launcher and installed uninstaller.
The ZIP itself uses the published checksum; its contained executable is signed.
Matching hashes identify the selected bytes. Signing does not complete product
qualification or guarantee SmartScreen reputation. Follow your organization's
execution policy and stop if signature or checksum validation fails.

## Install and select roots

Run the setup EXE and choose **Current user** or **All users**. Current user
installs by default to `%LOCALAPPDATA%\Programs\VCP`, asks for a separate private
data directory (default `%LOCALAPPDATA%\VCP`), and adds the program directory to
that user's PATH. All users requires administrator approval, installs under
`%ProgramFiles%\VCP`, and adds the program directory to the machine PATH. Each
Windows account uses its own local `%LOCALAPPDATA%\VCP` data directory in this
mode; setup does not create or share a common data directory. The shared program
directory must stay under Program Files. Setup refuses an existing program
directory it does not own. A Start menu shortcut is optional. Open a new terminal
after installation to use `vcp` by name.

Use the stable `vcp.exe` in the selected program directory:

```powershell
$vcp = Join-Path $env:LOCALAPPDATA 'Programs\VCP\vcp.exe'
& $vcp --version
$installation = & $vcp --resolve-installation | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Installation identity could not be resolved.' }
$installation
```

For an all-users installation, set `$vcp = Join-Path $env:ProgramFiles 'VCP\vcp.exe'`;
for a custom program directory, use its absolute path. The
resolver reports the selected versioned engine and the calling account's private
data directory. Use the stable
launcher for ordinary CLI commands. It selects the active verified payload and
inherits console input/output. An explicit CLI `--data-dir` selects a different
data root, so only use it intentionally. Never edit `active.json`, ownership
markers or release manifests to repair a failed selection.

Continue with [provider setup and the first task](beta-onboarding.md). Its examples
select the launcher explicitly. Provider setup and tasks require separate
budget admission; installation performs no inference and acquires no model.

## Connect VS Code

In VS Code 1.138.0, use **Extensions: Install from VSIX** and select the matching
`vcp-0.2.2-win32-x64.vsix`. Open a local initialized VCP workspace. Follow the
VSIX's included README for its connection and execution-profile dialogs.

Set `vcp.engineExecutable` and `vcp.dataDirectory` explicitly in **User** settings.
Use the native executable and data directory reported above. Workspace settings
cannot select an engine. Keep credentials in
the extension's credential input, never in settings or profile JSON. Recheck the
selection after upgrade or rollback. Grant VS Code workspace trust and VCP engine
trust explicitly before starting work; connection and reload do not grant control
or resume a task automatically.

## Portable use and optional local models

The ZIP contains the native `vcp.exe`, assets, notices, user documentation and
maintenance scripts. Extract it to a new private program directory and invoke
that native executable directly. `vcp-launch.exe` is the setup integration helper;
it requires setup's owned `engine` tree. Portable use does not register an
uninstaller. Keep its data outside the extracted payload.

Local embedding assets are deliberately separate. To acquire them, first review
the packaged `models/minilm-assets.json` specification and choose a new private
destination outside the payload. For a registered installation, locate the tools
beside the resolved engine:

```powershell
$payload = Split-Path -Parent $installation.executable
$models = Join-Path $installation.data_directory 'models\minilm'
& (Join-Path $payload 'tools\package-models.ps1') -Action Acquire -Destination $models
```

Run this in PowerShell 7. For portable use, set `$payload` to the extracted root
and `$models` to your chosen private destination. Acquisition is an explicit
network download. Use `-Action Verify` for an existing directory; corrupted or
incomplete assets are refused. Pass the verified directory with `--assets $models`
to `memory build` and `memory query` for an initialized workspace. Inspect
`& $vcp memory build --help` and `& $vcp memory query --help` for their arguments.

See [upgrade, rollback and recovery](beta-recovery.md) before changing versions.
