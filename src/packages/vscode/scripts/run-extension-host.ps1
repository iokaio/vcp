# SPDX-License-Identifier: Apache-2.0
param([Parameter(Mandatory=$true)][string]$InputFile)
$ErrorActionPreference='Stop'
if(-not ('VcpEditorQualificationNative' -as [type])) {
  Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class VcpEditorQualificationNative { [DllImport("kernel32.dll")] public static extern uint SetErrorMode(uint mode); [DllImport("advapi32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool IsTokenRestricted(IntPtr token); }'
}
$identity=[System.Security.Principal.WindowsIdentity]::GetCurrent()
try {
  if([VcpEditorQualificationNative]::IsTokenRestricted($identity.Token)) { throw 'Native editor qualification requires a normal user token; rerun with require_escalated before launching the editor.' }
} finally { $identity.Dispose() }
$inputSpec=Get-Content -LiteralPath $InputFile -Raw | ConvertFrom-Json
$env:VCP_EXTENSION_TEST_INPUT=(Resolve-Path -LiteralPath $InputFile).Path
Remove-Item Env:ELECTRON_RUN_AS_NODE -ErrorAction SilentlyContinue
Remove-Item Env:VSCODE_DEV -ErrorAction SilentlyContinue
# Versioned installations keep native runtime DLLs below the executable. Scope
# the matching runtime search path to this owned test process only.
$codeItem=Get-Item -LiteralPath $inputSpec.code
$runtimeRoots=@(@($codeItem.Directory)+@(Get-ChildItem -LiteralPath $codeItem.DirectoryName -Directory) | Where-Object {
  $manifest=Join-Path $_.FullName 'resources/app/package.json'
  (Test-Path -LiteralPath $manifest) -and ((Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json).version -eq $codeItem.VersionInfo.ProductVersion)
})
if($runtimeRoots.Count -ne 1) { throw 'Exactly one matching editor runtime is required' }
$runtimeRoot=$runtimeRoots[0].FullName
foreach($relative in @('ffmpeg.dll','libEGL.dll','libGLESv2.dll','icudtl.dat','v8_context_snapshot.bin','resources/app/out/main.js','resources/app/out/vs/workbench/workbench.desktop.main.js')) {
  if(-not (Test-Path -LiteralPath (Join-Path $runtimeRoot $relative) -PathType Leaf)) { throw "Required editor runtime asset missing: $relative" }
}
$env:PATH="$runtimeRoot;$env:PATH"
$runtimeFiles=@($codeItem)+@(Get-ChildItem -LiteralPath $runtimeRoot -File -Filter '*.dll')
@{version=$codeItem.VersionInfo.ProductVersion;runtime=$runtimeRoot;files=@($runtimeFiles | ForEach-Object { @{name=$_.Name;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash} })} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $inputSpec.runtimeEvidence
$sharedData=Join-Path $inputSpec.userData 'shared-data'
New-Item -ItemType Directory -Force -Path $sharedData | Out-Null
$launchArguments=@('--new-window','--skip-welcome','--skip-release-notes','--skip-add-to-recently-opened','--disable-updates','--disable-gpu',"--user-data-dir=`"$($inputSpec.userData)`"","--shared-data-dir=`"$sharedData`"","--extensions-dir=`"$($inputSpec.extensions)`"")
# Development windows do not register a durable backup path in the pinned editor.
# Dirty-buffer reload qualification therefore uses ordinary extensions copied into
# the fixture's private extensions directory, with no development-host switches.
if($inputSpec.installed -ne $true) {
  $launchArguments+=@('--disable-extensions',"--extensionDevelopmentPath=`"$($inputSpec.extension)`"","--extensionDevelopmentPath=`"$($inputSpec.driver)`"")
}
$launchArguments+="`"$($inputSpec.workspaceFile)`""
# Children inherit this owned launcher's error mode. Failed native startup must
# report through the exit status/logs, not interrupt the user with system dialogs.
$previousErrorMode=[VcpEditorQualificationNative]::SetErrorMode(3)
[void][VcpEditorQualificationNative]::SetErrorMode($previousErrorMode -bor 3)
$process=$null
try {
  $process=Start-Process -FilePath $inputSpec.code -ArgumentList $launchArguments -WorkingDirectory $inputSpec.userData -WindowStyle Hidden -PassThru -RedirectStandardOutput $inputSpec.stdout -RedirectStandardError $inputSpec.stderr
  if(-not $process.WaitForExit(120000)) { $process.Kill($true); throw 'Owned extension-host deadline exceeded' }
  if($process.ExitCode -ne 0) { throw "Extension host failed with exit $($process.ExitCode)" }
  if(-not (Test-Path -LiteralPath $inputSpec.result)) { throw 'Extension-host result unavailable' }
} finally {
  [void][VcpEditorQualificationNative]::SetErrorMode($previousErrorMode)
  if($process -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit(5000) | Out-Null }
  $editorLogs=Join-Path $inputSpec.userData 'logs'
  if(Test-Path -LiteralPath $editorLogs) { Copy-Item -LiteralPath $editorLogs -Destination $inputSpec.diagnostics -Recurse -Force }
}
