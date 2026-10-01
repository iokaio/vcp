# Set up VCP Local for the internal beta

Use the matching Windows x64 installer from [VCP beta downloads](https://downloads.ioka.io/)
and the `iokaio.vcp-local` pre-release extension (`vcp-local-0.2.2-win32-x64.vsix`
for manual installation)
with VS Code **1.138.0**. This version requires the signed Ioka LLC native candidate;
construction and qualification are in progress. The previously published beta.1
remains unsigned. The candidate record
identifies the native `0.2.0-beta.2`, SDK `0.2.2`, exact artifact hashes, signature checks and
remaining qualification limits. Installing the extension does not install the engine.

If replacing the earlier `vcp.vcp-local` extension, disconnect and uninstall it
first, then install `iokaio.vcp-local`. Retain the native installation, data
directory and User settings, and reconnect explicitly. The new publisher identity
does not migrate the old extension's local connection state.

## Select the installed engine and data directory

Complete the native per-user setup first. In PowerShell, resolve the active
installation using the stable launcher at your selected installation root:

```powershell
$launcher = Join-Path $env:LOCALAPPDATA 'Programs\VCP\vcp.exe'
$installed = & $launcher --resolve-installation | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or $installed.schema -ne 'vcp-installed-engine/1') {
  throw 'The installed engine could not be resolved.'
}
$installed | Select-Object executable, data_directory
```

For a custom installation root, replace `$launcher` with its absolute path.
Keep the selected data directory outside repositories and synchronized folders.
It contains local history and must be retained across updates.

Open **Preferences: Open User Settings (JSON)**. Set these two values using the
reported paths, with JSON backslashes escaped. For example, this command prints
the correctly escaped values for you to review and copy:

```powershell
@{ 'vcp.engineExecutable' = $installed.executable;
   'vcp.dataDirectory' = $installed.data_directory } | ConvertTo-Json
```

The executable setting must name the resolved versioned **vcp.exe**. The stable
launcher is for terminal use; it does not host editor connections. The extension
reads only explicit User settings. Project and workspace overrides are ignored.
Do not add credentials or execution profiles to settings.

## Initialize a workspace and its execution profile

Follow the **beta-onboarding.md** walkthrough in `docs/usage` beside the resolved
versioned engine named by `$installed.executable`. The same guide is available in
[the repository](https://github.com/iokaio/vcp/blob/main/docs/usage/beta-onboarding.md).
It covers the exact `setup provider`, `setup profile` and `setup check` commands,
credential entry, renewal, policy and budgets. Select the same data directory for
native commands, using `--data-dir` when needed.

Provider setup makes at most two paid probe calls within the cap you explicitly
authorize. Each task has a separate cap; neither cap is an account-wide limit.
Do not repeat a probe with unresolved billing liability. Profile creation and
`setup check` are offline. A new workspace gets its own explicit `--config`
profile outside the workspace. `--trust-workspace` is a deliberate trust grant.

The first accepted native task registers the workspace and durable history.
Running the CLI without a task only discovers unfinished work; it does not
initialize a new workspace. Complete the walkthrough's cited README read task
before connecting the editor. Keep its profile filename and configured task
budget, maximum requests and deadline for the editor's execution dialog.

## Connect and start work deliberately

1. Install the supplied VSIX using **Extensions: Install from VSIX** and reload
   when requested. Open the initialized local workspace folder.
2. Open the VCP activity-bar view and choose **Connect…**. Select the full folder
   URI. Verify its engine path, workspace/root identity, data and trust state.
   The initial connection observes history. It makes no model request.
3. Review VS Code workspace trust and VCP engine trust separately. Restricted
   Mode supports observation and the setup guide; it does not authorize tasks.
4. To run an editor task, use **VCP: Start Execution-backed Task**. Select the
   workspace's trusted profile, objective and the exact configured cap, request
   limit and deadline. A generated setup profile uses eight requests and a
   300-second deadline. Enter the provider credential in the masked dialog.
   The credential is used for that launch and is not stored in settings or
   workspace/webview state. Close another engine owner and allow its idle
   shutdown before requesting controller ownership.
5. Review policy questions and editor changes explicitly. A task, an approval,
   controller acquisition and resume are distinct actions. Dirty-buffer edits
   require current version-bound review; VCP does not save the buffer for you.

Reload restores observation only. It never resumes a task, reacquires control,
replays an uncertain command or reapplies an edit. Inspect retained command
outcomes before issuing new mutations.

## Repair, update and support

For missing workspace/profile errors, verify the matching native workspace,
data directory and `--config`, then run `setup check`. Expired provider metadata
requires a newly qualified generation and new profile; select it explicitly.
Changing a base profile or its imported preferences requires a new execution
connection. Never alter receipt hashes, prices or expiry to make a profile pass.

For an update, disconnect, close active engine owners, install the matching
reviewed native/VSIX pair, resolve the launcher again and explicitly update both
User settings. Preserve history and recovery material. Reconnect and inspect
identity and pending outcomes. See [compatibility and uninstall](COMPATIBILITY.md).

When reporting a failure, retain the candidate manifest and hashes, native/VSIX
versions, failing step and static error category. Share only reviewed, redacted
diagnostics. Never include API keys, private profiles, task content or history
unless separately reviewed for disclosure. The native packaged known-issues
and recovery guides describe the remaining internal beta qualification limits.
