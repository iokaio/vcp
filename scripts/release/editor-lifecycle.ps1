# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$NativeResult,[Parameter(Mandatory)][string]$SetupResult,
    [Parameter(Mandatory)][string]$VsixManifest,[Parameter(Mandatory)][string]$Code,
    [Parameter(Mandatory)][string]$Workspace,[Parameter(Mandatory)][string]$DataRoot,
    [Parameter(Mandatory)][string]$Scope,[Parameter(Mandatory)][string]$Task,
    [Parameter(Mandatory)][string]$OutputRoot,
    [Parameter(Mandatory)][ValidateSet('install','restart','failed-update','missing','reconnect')][string]$Mode
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'candidate-runtime.ps1')
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$root=[IO.Path]::GetFullPath($OutputRoot)
if ($root.Equals($repo,[StringComparison]::OrdinalIgnoreCase) -or $root.StartsWith($repo+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Private lifecycle root must be outside checkout' }
$node=(Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$pwsh=(Get-Command pwsh -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$native=Get-Content -LiteralPath $NativeResult -Raw | ConvertFrom-Json
$setup=Get-Content -LiteralPath $SetupResult -Raw | ConvertFrom-Json
$vsix=Get-Content -LiteralPath $VsixManifest -Raw | ConvertFrom-Json
$nativeHash=(Get-FileHash -LiteralPath $NativeResult).Hash.ToLowerInvariant()
if ($native.status -cne 'release-candidate' -or $setup.schema -cne 'vcp-setup-result/1' -or $vsix.schema -cne 'vcp-vsix-package/1' -or $setup.candidate_id -cne $native.manifest.release.candidate_id -or $setup.native_archive_sha256 -cne $native.archive_sha256 -or $vsix.archive.file -match '[\\/:]' -or $native.package -match '[\\/:]' -or $setup.archive.file -match '[\\/:]' -or $vsix.engine.native_archive_sha256 -cne $native.archive_sha256 -or $vsix.release.candidate_id -cne $native.manifest.release.candidate_id -or $vsix.engine.native_manifest_sha256 -cne $nativeHash) { throw 'Exact matching strict candidate receipts required' }
$archive=Join-Path (Split-Path -Parent $VsixManifest) $vsix.archive.file
$zip=Join-Path (Split-Path -Parent $NativeResult) $native.package
$installer=Join-Path (Split-Path -Parent $SetupResult) $setup.archive.file
if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne $vsix.archive.sha256 -or (Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant() -cne $native.archive_sha256 -or (Get-FileHash -LiteralPath $installer).Hash.ToLowerInvariant() -cne $setup.archive.sha256) { throw 'Final artifact bytes changed' }
$editorLayout=Resolve-BetaEditor -Code $Code
$Code=$editorLayout.code; $editorRoot=$editorLayout.root
$tools=Get-Content -LiteralPath (Join-Path $repo 'release/candidate-tools.json') -Raw | ConvertFrom-Json
$editorCommit=$editorLayout.commit
$scopeValue=$Scope | ConvertFrom-Json
$binding=[ordered]@{native=$nativeHash;setup=(Get-FileHash -LiteralPath $SetupResult).Hash.ToLowerInvariant();vsix=(Get-FileHash -LiteralPath $VsixManifest).Hash.ToLowerInvariant();editor=(Get-FileHash -LiteralPath $Code).Hash.ToLowerInvariant();editor_commit=$editorCommit;workspace=[IO.Path]::GetFullPath($Workspace);data=[IO.Path]::GetFullPath($DataRoot);scope=$scopeValue;task=$Task} | ConvertTo-Json -Depth 12 -Compress
$progressPath=Join-Path $root 'lifecycle.json'
$lastReport=Join-Path $root 'last-report.json'
$modes=@('install','restart','failed-update','missing','reconnect')
$user=Join-Path $root 'user'; $extensions=Join-Path $root 'extensions'; $driver=Join-Path $root 'driver'
$workspaceFile=Join-Path $root 'candidate.code-workspace'
$launch=Join-Path $root 'run-extension-host.ps1'
$contract=Join-Path $PSScriptRoot 'editor-lifecycle.cjs'
$editorCli=$editorLayout.cli
$editorEnvironment=@{ELECTRON_RUN_AS_NODE='1';PATH="$($editorLayout.runtime);$env:SystemRoot\System32;$env:SystemRoot"}
$base=@($editorCli,'--user-data-dir',$user,'--extensions-dir',$extensions)
function Installed-Extension {
    $installedCandidates=@(Get-ChildItem -LiteralPath $extensions -Directory | Where-Object {
        $manifest=Join-Path $_.FullName 'package.json'
        if (Test-Path -LiteralPath $manifest) { $package=Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json; $package.publisher -ceq 'vcp' -and $package.name -ceq 'vcp-local' -and $package.version -ceq $vsix.extension.version }
    })
    if ($installedCandidates.Count -ne 1) { throw 'Exactly one original candidate extension must remain installed' }
    return $installedCandidates[0].FullName
}
function Extension-Inventory {
    return ((Invoke-BetaProcess $node @($contract,'verified-inventory',(Installed-Extension),$VsixManifest) $root).stdout | ConvertFrom-Json)
}
if ($Mode -ceq 'install') {
    $installed=Install-BetaCandidate $NativeResult $SetupResult $root $DataRoot
    # Keep this marker and registered program root if any later observation fails.
    $progress=@{schema='vcp-editor-lifecycle-private/1';binding=$binding;mode='prepared';inventory=$null}
    $progress | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $progressPath -Encoding utf8NoBOM
    foreach ($directory in @((Join-Path $user 'User'),$extensions,$driver)) { New-Item -ItemType Directory -Path $directory -Force | Out-Null }
    @{'security.workspace.trust.enabled'=$true;'security.workspace.trust.startupPrompt'='never';'update.mode'='none';'extensions.autoUpdate'=$false;'extensions.autoCheckUpdates'=$false;'telemetry.telemetryLevel'='off';'workbench.startupEditor'='none';'files.autoSave'='off'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $user 'User/settings.json') -Encoding utf8NoBOM
    @{folders=@(@{path=$Workspace})} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $workspaceFile -Encoding utf8NoBOM
    $null=Invoke-BetaProcess $Code ($base+@('--install-extension',$archive,'--force')) $root $editorEnvironment
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'editor-lifecycle-driver.cjs') -Destination (Join-Path $driver 'driver.cjs')
    Copy-Item -LiteralPath (Join-Path $repo 'src/packages/vscode/tests/package-host.cjs') -Destination (Join-Path $driver 'package-host.cjs')
    @{name='candidate-lifecycle-driver';publisher='vcp-test';version='0.0.1';engines=@{vscode='1.138.0'};activationEvents=@('*');main='./driver.cjs';files=@('driver.cjs','package-host.cjs');extensionKind=@('workspace');capabilities=@{untrustedWorkspaces=@{supported=$true}}} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $driver 'package.json') -Encoding utf8NoBOM
    $driverArchive=Join-Path $root 'driver.vsix'
    $null=Invoke-BetaProcess $node @((Join-Path $repo 'src/packages/vscode/node_modules/@vscode/vsce/vsce'),'package','--no-dependencies','--allow-missing-repository','--skip-license','--out',$driverArchive) $driver
    $null=Invoke-BetaProcess $Code ($base+@('--install-extension',$driverArchive,'--force')) $root $editorEnvironment
    Copy-Item -LiteralPath (Join-Path $repo 'src/packages/vscode/scripts/run-extension-host.ps1') -Destination $launch
    $progress.inventory=(Extension-Inventory).sha256
    $progress | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $progressPath -Encoding utf8NoBOM
} else {
    $progress=Get-Content -LiteralPath $progressPath -Raw | ConvertFrom-Json
    $index=[Array]::IndexOf($modes,$Mode)
    if ($progress.schema -cne 'vcp-editor-lifecycle-private/1' -or $progress.binding -cne $binding -or $progress.mode -cne $modes[$index-1]) { throw 'Lifecycle inputs changed or observations are out of order' }
    $app=Join-Path $root 'Program Files café'
    $installed=@{app=$app;launcher=(Join-Path $app 'vcp.exe');engine=(Join-Path $app "engine/releases/$($native.archive_sha256)/vcp.exe");data=$DataRoot;registration='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VCP.InternalBeta.1_is1'}
}
# Check the selected production bytes and data identity before EVERY editor case.
$registered=(Get-ItemProperty -LiteralPath $installed.registration).InstallLocation
if ([IO.Path]::GetFullPath($registered).TrimEnd('\') -ine [IO.Path]::GetFullPath($installed.app).TrimEnd('\')) { throw 'Registered installation no longer belongs to this private lifecycle run' }
$expected=@($native.manifest.files | Where-Object path -ceq 'vcp.exe')
if ($expected.Count -ne 1 -or (Get-FileHash -LiteralPath $installed.launcher).Hash.ToLowerInvariant() -cne $setup.launcher_sha256 -or (Get-FileHash -LiteralPath $installed.engine).Hash.ToLowerInvariant() -cne $expected[0].sha256) { throw 'Installed native bytes differ from strict candidate' }
$null=Invoke-BetaProcess $node @((Join-Path $repo 'scripts/evals/production-package.cjs'),$NativeResult,(Split-Path -Parent $installed.engine)) $root
$selection=(Invoke-BetaProcess $installed.launcher @('--resolve-installation') $root).stdout | ConvertFrom-Json
if ($selection.schema -cne 'vcp-installed-engine/1' -or [IO.Path]::GetFullPath($selection.executable).Replace('\\?\','') -ine $installed.engine -or [IO.Path]::GetFullPath($selection.data_directory).Replace('\\?\','').TrimEnd('\') -ine [IO.Path]::GetFullPath($DataRoot).TrimEnd('\')) { throw 'Installed engine/data identity changed' }
if ((Extension-Inventory).sha256 -cne $progress.inventory) { throw 'Original installed VSIX files changed' }
$rejection=$null
if ($Mode -ceq 'failed-update') {
    $corrupt=Join-Path $root 'truncated-final-candidate.vsix'
    $source=[IO.File]::OpenRead($archive)
    try { $prefix=[byte[]]::new(16); if ($source.Read($prefix,0,$prefix.Length) -ne $prefix.Length) { throw 'Final VSIX is unexpectedly short' }; [IO.File]::WriteAllBytes($corrupt,$prefix) } finally { $source.Dispose() }
    $rejected=Invoke-BetaProcess $Code ($base+@('--install-extension',$corrupt,'--force')) $root $editorEnvironment 90 1
    $listed=Invoke-BetaProcess $Code ($base+@('--list-extensions','--show-versions')) $root $editorEnvironment
    if (@($listed.stdout -split '\r?\n' | Where-Object { $_.Trim() -ceq ('vcp.vcp-local@'+$vsix.extension.version) }).Count -ne 1 -or (Extension-Inventory).sha256 -cne $progress.inventory -or (Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne $vsix.archive.sha256) { throw 'Rejected update changed the original installed or archived VSIX' }
    $rejection=@{exit_code=$rejected.exit_code;retained_version=$vsix.extension.version;installed_inventory_sha256=$progress.inventory;truncated_sha256=(Get-FileHash -LiteralPath $corrupt).Hash.ToLowerInvariant();original_archive_preserved=$true}
}
$engine=if ($Mode -ceq 'missing') { Join-Path $root 'missing-engine.exe' } else { $installed.engine }
if ($Mode -ceq 'missing' -and (Test-Path -LiteralPath $engine)) { throw 'Missing-engine fixture is not absent' }
$inputFile=Join-Path $root ($Mode+'-input.json'); $resultFile=Join-Path $root ($Mode+'-result.json')
if (Test-Path -LiteralPath $resultFile) { throw 'Fresh observation result required' }
@{restrictedPath=$true;code=$Code;editorRoot=$editorRoot;installed=$true;workspaceFile=$workspaceFile;userData=$user;extensions=$extensions;result=$resultFile;stdout=(Join-Path $root ($Mode+'-stdout.log'));stderr=(Join-Path $root ($Mode+'-stderr.log'));diagnostics=(Join-Path $root ($Mode+'-editor-logs'));runtimeEvidence=(Join-Path $root ($Mode+'-runtime.json'));workspace=$Workspace;data=$DataRoot;executable=$engine;scope=$scopeValue;task=$Task;mode=$Mode;version=$vsix.extension.version;checkout=$repo;reloadMarker=(Join-Path $root 'reload.json')} | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $inputFile -Encoding utf8NoBOM
$null=Invoke-BetaProcess $pwsh @('-NoProfile','-File',$launch,'-InputFile',$inputFile) $root @{} 150
$previous=if ($Mode -ceq 'install') { '-' } else { $lastReport }
$report=(Invoke-BetaProcess $node @($contract,'observation',$inputFile,$resultFile,$previous) $root).stdout | ConvertFrom-Json
if ((Extension-Inventory).sha256 -cne $progress.inventory) { throw 'Editor lifecycle changed original installed VSIX files' }
$report | Add-Member -NotePropertyMembers @{source_commit=$native.manifest.release.reviewed_commit;source_content_sha256=$native.manifest.release.source_content_sha256;native_sha256=$native.archive_sha256;setup_sha256=$setup.archive.sha256;vsix_sha256=$vsix.archive.sha256;engine_sha256=$expected[0].sha256;native_payload_verified=$true;editor_version=$tools.editor.version;editor_commit=$editorCommit;editor_executable_sha256=(Get-FileHash -LiteralPath $Code).Hash.ToLowerInvariant();installed_inventory_sha256=$progress.inventory;malformed_update=$rejection;limitations=@('Synthetic retained paused history; no first useful task, reviewed edit, provider call or real accounting work.','Same final VSIX is truncated for rejection; no successful distinct-version upgrade or rollback.','Extension-host assertions; no human UI or clean-machine usability claim.')}
if ($Mode -ceq 'reconnect') {
    # Normal observers release the native owner after its 30-second idle grace.
    Start-Sleep -Seconds 32
    $null=Invoke-BetaProcess $Code ($base+@('--uninstall-extension','vcp.vcp-local')) $root $editorEnvironment
    $listed=Invoke-BetaProcess $Code ($base+@('--list-extensions','--show-versions')) $root $editorEnvironment
    if (@($listed.stdout -split '\r?\n' | Where-Object { $_ -match '^vcp\.vcp-local@' }).Count) { throw 'VCP extension remains installed after removal' }
    Uninstall-BetaCandidate $installed $root
    $report | Add-Member -NotePropertyName uninstalled -NotePropertyValue $true
}
$progress.mode=$Mode
$progress | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $progressPath -Encoding utf8NoBOM
$report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $lastReport -Encoding utf8NoBOM
$report | ConvertTo-Json -Depth 12
