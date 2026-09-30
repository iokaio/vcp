# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$NativeResult,[Parameter(Mandatory)][string]$SetupResult,
    [Parameter(Mandatory)][string]$VsixManifest,[Parameter(Mandatory)][string]$Code,
    [Parameter(Mandatory)][string]$Workspace,[Parameter(Mandatory)][string]$DataRoot,
    [Parameter(Mandatory)][string]$OutputRoot
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'candidate-runtime.ps1')
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$root=[IO.Path]::GetFullPath($OutputRoot)
$node=(Get-Command node -CommandType Application).Source
$pwsh=(Get-Command pwsh -CommandType Application).Source
$native=Get-Content -LiteralPath $NativeResult -Raw | ConvertFrom-Json
$vsix=Get-Content -LiteralPath $VsixManifest -Raw | ConvertFrom-Json
if ($vsix.archive.file -match '[\\/:]' -or $vsix.engine.native_archive_sha256 -cne $native.archive_sha256 -or $vsix.release.candidate_id -cne $native.manifest.release.candidate_id -or $vsix.engine.native_manifest_sha256 -cne (Get-FileHash -LiteralPath $NativeResult).Hash.ToLowerInvariant()) { throw 'VSIX does not bind this exact native candidate' }
$archive=Join-Path (Split-Path -Parent $VsixManifest) $vsix.archive.file
if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne $vsix.archive.sha256) { throw 'VSIX bytes changed' }
$editorRoot=Split-Path -Parent $Code
if ((Get-Content -LiteralPath (Join-Path $editorRoot 'resources/app/package.json') -Raw | ConvertFrom-Json).version -cne '1.138.0') { throw 'Exact supported editor required' }
$installed=$null
try {
    $installed=Install-BetaCandidate $NativeResult $SetupResult $root $DataRoot
    $user=Join-Path $root 'user'; $extensions=Join-Path $root 'extensions'; $driver=Join-Path $root 'driver'
    foreach ($dir in @((Join-Path $user 'User'),$extensions,$driver)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    @{ 'security.workspace.trust.enabled'=$true; 'security.workspace.trust.startupPrompt'='never'; 'update.mode'='none'; 'extensions.autoUpdate'=$false; 'extensions.autoCheckUpdates'=$false; 'telemetry.telemetryLevel'='off'; 'workbench.startupEditor'='none'; 'files.autoSave'='off' } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $user 'User/settings.json') -Encoding utf8NoBOM
    $workspaceFile=Join-Path $root 'candidate.code-workspace'
    @{folders=@(@{path=$Workspace})} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $workspaceFile -Encoding utf8NoBOM
    $editorCli=Join-Path $editorRoot 'resources/app/out/cli.js'
    $editorEnvironment=@{ELECTRON_RUN_AS_NODE='1';PATH="$editorRoot;$env:SystemRoot\System32;$env:SystemRoot"}
    $base=@($editorCli,'--user-data-dir',$user,'--extensions-dir',$extensions)
    $null=Invoke-BetaProcess $Code ($base+@('--install-extension',$archive,'--force')) $root $editorEnvironment
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'candidate-editor-driver.cjs') -Destination (Join-Path $driver 'driver.cjs')
    @{name='candidate-observer-driver';publisher='vcp-test';version='0.0.1';engines=@{vscode='1.138.0'};activationEvents=@('*');main='./driver.cjs';extensionKind=@('workspace');capabilities=@{untrustedWorkspaces=@{supported=$true}}} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $driver 'package.json') -Encoding utf8NoBOM
    $driverArchive=Join-Path $root 'driver.vsix'
    $null=Invoke-BetaProcess $node @((Join-Path $repo 'src/packages/vscode/node_modules/@vscode/vsce/vsce'),'package','--no-dependencies','--allow-missing-repository','--skip-license','--out',$driverArchive) $driver
    $null=Invoke-BetaProcess $Code ($base+@('--install-extension',$driverArchive,'--force')) $root $editorEnvironment
    $inputFile=Join-Path $root 'input.json'; $resultFile=Join-Path $root 'result.json'
    @{restrictedPath=$true;code=$Code;editorRoot=$editorRoot;installed=$true;workspaceFile=$workspaceFile;userData=$user;extensions=$extensions;result=$resultFile;stdout=(Join-Path $root 'editor.stdout.log');stderr=(Join-Path $root 'editor.stderr.log');diagnostics=(Join-Path $root 'editor-diagnostics');runtimeEvidence=(Join-Path $root 'editor-runtime.json');workspace=$Workspace;data=$DataRoot;executable=$installed.engine;version=$vsix.extension.version;checkout=$repo} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $inputFile -Encoding utf8NoBOM
    $launch=Join-Path $root 'run-extension-host.ps1'
    Copy-Item -LiteralPath (Join-Path $repo 'src/packages/vscode/scripts/run-extension-host.ps1') -Destination $launch
    $null=Invoke-BetaProcess $pwsh @('-NoProfile','-File',$launch,'-InputFile',$inputFile) $root @{} 150
    $result=Get-Content -LiteralPath $resultFile -Raw | ConvertFrom-Json
    if ($result.status -cne 'pass' -or $result.observer -ne $true) { throw ('Installed observer failed: '+($result | ConvertTo-Json -Compress -Depth 8)) }
    # Normal observer owners retain a 30-second idle grace; allow orderly release
    # before the setup uninstaller verifies exclusive engine ownership.
    Start-Sleep -Seconds 32
    $null=Invoke-BetaProcess $Code ($base+@('--uninstall-extension','vcp.vcp-local')) $root $editorEnvironment
    $result | Add-Member -NotePropertyMembers @{native_sha256=$native.archive_sha256;setup_sha256=$installed.setup_sha256;vsix_sha256=$vsix.archive.sha256;engine_sha256=$installed.engine_sha256;editor_version='1.138.0';editor_executable_sha256=(Get-FileHash -LiteralPath $Code).Hash.ToLowerInvariant();limitations=@('Synthetic retained paused history; no first useful task or live provider call.','Hosted Windows image, not clean standard-user qualification.','No distinct-build update, rollback, reviewed edit or reload asserted.')}
} finally { if ($installed) { Uninstall-BetaCandidate $installed $root } }
$result | ConvertTo-Json -Depth 8
