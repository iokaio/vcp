# SPDX-License-Identifier: Apache-2.0
param([Parameter(Mandatory=$true)][string]$InputFile)
$ErrorActionPreference='Stop'
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
$env:PATH="$runtimeRoot;$env:PATH"
$runtimeFiles=@($codeItem)+@(Get-ChildItem -LiteralPath $runtimeRoot -File -Filter '*.dll')
@{version=$codeItem.VersionInfo.ProductVersion;runtime=$runtimeRoot;files=@($runtimeFiles | ForEach-Object { @{name=$_.Name;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash} })} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $inputSpec.runtimeEvidence
$launchArguments=@('--new-window','--disable-extensions','--skip-welcome','--skip-release-notes','--skip-add-to-recently-opened','--disable-updates','--disable-gpu',"--user-data-dir=`"$($inputSpec.userData)`"","--extensions-dir=`"$($inputSpec.extensions)`"","--extensionDevelopmentPath=`"$($inputSpec.extension)`"","--extensionTestsPath=`"$($inputSpec.runner)`"","`"$($inputSpec.workspaceFile)`"")
$process=Start-Process -FilePath $inputSpec.code -ArgumentList $launchArguments -WorkingDirectory $inputSpec.userData -WindowStyle Hidden -PassThru -RedirectStandardOutput $inputSpec.stdout -RedirectStandardError $inputSpec.stderr
try {
  if(-not $process.WaitForExit(120000)) { $process.Kill($true); throw 'Owned extension-host deadline exceeded' }
  if($process.ExitCode -ne 0) { throw "Extension host failed with exit $($process.ExitCode)" }
  if(-not (Test-Path -LiteralPath $inputSpec.result)) { throw 'Extension-host result unavailable' }
} finally {
  if(-not $process.HasExited) { $process.Kill($true); $process.WaitForExit(5000) | Out-Null }
  $editorLogs=Join-Path $inputSpec.userData 'logs'
  if(Test-Path -LiteralPath $editorLogs) { Copy-Item -LiteralPath $editorLogs -Destination $inputSpec.diagnostics -Recurse -Force }
}
