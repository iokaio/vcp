# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$NativeResult,[Parameter(Mandatory)][string]$SetupResult,
    [Parameter(Mandatory)][string]$VsixManifest,[Parameter(Mandatory)][string]$Code,
    [Parameter(Mandatory)][string]$Workspace,[Parameter(Mandatory)][string]$DataRoot,
    [Parameter(Mandatory)][string]$Scope,[Parameter(Mandatory)][string]$Task,
    [Parameter(Mandatory)][string]$OutputRoot,
    [Parameter(Mandatory)][ValidateSet('restricted','trusted','finish')][string]$Mode
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'candidate-runtime.ps1')
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$root=[IO.Path]::GetFullPath($OutputRoot)
if ($root.Equals($repo,[StringComparison]::OrdinalIgnoreCase) -or $root.StartsWith($repo+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Private refusal root must be outside checkout' }
$node=(Get-Command node -CommandType Application | Select-Object -First 1).Source
$pwsh=(Get-Command pwsh -CommandType Application | Select-Object -First 1).Source
$native=Get-Content -LiteralPath $NativeResult -Raw | ConvertFrom-Json
$setup=Get-Content -LiteralPath $SetupResult -Raw | ConvertFrom-Json
$vsix=Get-Content -LiteralPath $VsixManifest -Raw | ConvertFrom-Json
$nativeHash=(Get-FileHash -LiteralPath $NativeResult).Hash.ToLowerInvariant()
if ($native.status -cne 'release-candidate' -or $setup.schema -cne 'vcp-setup-result/1' -or $vsix.schema -cne 'vcp-vsix-package/1' -or $setup.candidate_id -cne $native.manifest.release.candidate_id -or $setup.native_archive_sha256 -cne $native.archive_sha256 -or $vsix.archive.file -match '[\\/:]' -or $native.package -match '[\\/:]' -or $setup.archive.file -match '[\\/:]' -or $vsix.engine.native_archive_sha256 -cne $native.archive_sha256 -or $vsix.release.candidate_id -cne $native.manifest.release.candidate_id -or $vsix.engine.native_manifest_sha256 -cne $nativeHash) { throw 'Exact matching strict candidate receipts required' }
$archive=Join-Path (Split-Path -Parent $VsixManifest) $vsix.archive.file
$zip=Join-Path (Split-Path -Parent $NativeResult) $native.package
$installer=Join-Path (Split-Path -Parent $SetupResult) $setup.archive.file
if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne $vsix.archive.sha256 -or (Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant() -cne $native.archive_sha256 -or (Get-FileHash -LiteralPath $installer).Hash.ToLowerInvariant() -cne $setup.archive.sha256) { throw 'Final artifact bytes changed' }
$editor=Resolve-BetaEditor -Code $Code
$Code=$editor.code
$scopeValue=$Scope | ConvertFrom-Json
$contract=Join-Path $PSScriptRoot 'editor-refusals.cjs'
$inventory=Join-Path $PSScriptRoot 'editor-lifecycle.cjs'
$progressPath=Join-Path $root 'refusals.json'
$extensions=Join-Path $root 'extensions'
$driver=Join-Path $root 'driver'
$launch=Join-Path $root 'run-extension-host.ps1'
$driverSource=Join-Path $PSScriptRoot 'editor-refusals-driver.cjs'
$driverHash=(Get-FileHash -LiteralPath $driverSource).Hash.ToLowerInvariant()
$binding=[ordered]@{native=$nativeHash;setup=(Get-FileHash -LiteralPath $SetupResult).Hash.ToLowerInvariant();vsix=(Get-FileHash -LiteralPath $VsixManifest).Hash.ToLowerInvariant();editor=$editor.code_sha256;editor_commit=$editor.commit;driver=$driverHash;workspace=[IO.Path]::GetFullPath($Workspace);data=[IO.Path]::GetFullPath($DataRoot);scope=$scopeValue;task=$Task} | ConvertTo-Json -Depth 12 -Compress
$editorEnvironment=@{ELECTRON_RUN_AS_NODE='1';PATH="$($editor.runtime);$env:SystemRoot\System32;$env:SystemRoot"}
$installerUser=Join-Path $root 'install-user'
$base=@($editor.cli,'--user-data-dir',$installerUser,'--extensions-dir',$extensions)
function Installed-Extension([string]$Publisher,[string]$Name,[string]$Version) {
    $found=@(Get-ChildItem -LiteralPath $extensions -Directory | Where-Object {
        $file=Join-Path $_.FullName 'package.json'
        if (Test-Path -LiteralPath $file) { $package=Get-Content -LiteralPath $file -Raw | ConvertFrom-Json; $package.publisher -ceq $Publisher -and $package.name -ceq $Name -and $package.version -ceq $Version }
    })
    if ($found.Count -ne 1) { throw 'Exactly one requested installed extension required' }
    return $found[0].FullName
}
function Verify-Payload {
    $registered=(Get-ItemProperty -LiteralPath $installed.registration).InstallLocation
    if ([IO.Path]::GetFullPath($registered).TrimEnd('\') -ine [IO.Path]::GetFullPath($installed.app).TrimEnd('\')) { throw 'Registered installation differs' }
    if ((Get-FileHash -LiteralPath $installed.launcher).Hash.ToLowerInvariant() -cne $setup.launcher_sha256) { throw 'Installed launcher bytes differ' }
    $null=Invoke-BetaProcess $node @((Join-Path $repo 'scripts/evals/production-package.cjs'),$NativeResult,(Split-Path -Parent $installed.engine)) $root
    $selection=(Invoke-BetaProcess $installed.launcher @('--resolve-installation') $root).stdout | ConvertFrom-Json
    if ($selection.schema -cne 'vcp-installed-engine/1' -or [IO.Path]::GetFullPath($selection.executable).Replace('\\?\','') -ine $installed.engine.Replace('\\?\','') -or [IO.Path]::GetFullPath($selection.data_directory).Replace('\\?\','').TrimEnd('\') -ine [IO.Path]::GetFullPath($DataRoot).TrimEnd('\')) { throw 'Installed engine/data selection differs' }
    $current=((Invoke-BetaProcess $node @($inventory,'verified-inventory',(Installed-Extension 'iokaio' 'vcp-local' $vsix.extension.version),$VsixManifest) $root).stdout | ConvertFrom-Json).sha256
    if ($progress.inventory -and $current -cne $progress.inventory) { throw 'Original installed VSIX changed' }
    $driverPath=Installed-Extension 'vcp-test' 'candidate-refusal-driver' '0.0.1'
    if ((Get-FileHash -LiteralPath (Join-Path $driverPath 'driver.cjs')).Hash.ToLowerInvariant() -cne $driverHash) { throw 'Installed driver changed' }
    return $current
}
if ($Mode -ceq 'restricted') {
    # Install-BetaCandidate may fail after registration; its entire root is kept.
    $installed=Install-BetaCandidate $NativeResult $SetupResult $root $DataRoot
    $progress=@{schema='vcp-editor-refusals-private/1';binding=$binding;mode='prepared';inventory=$null}
    $progress | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $progressPath -Encoding utf8NoBOM
    foreach ($directory in @($extensions,$driver,$installerUser,(Join-Path $root 'uninitialized'),(Join-Path $root 'wrong-data'),(Join-Path $root 'outside'))) { New-Item -ItemType Directory -Path $directory -Force | Out-Null }
    [IO.File]::WriteAllText((Join-Path $root 'outside/outside.txt'),"original human work`n",[Text.UTF8Encoding]::new($false))
    $null=Invoke-BetaProcess $Code ($base+@('--install-extension',$archive,'--force')) $root $editorEnvironment
    Copy-Item -LiteralPath $driverSource -Destination (Join-Path $driver 'driver.cjs')
    @{name='candidate-refusal-driver';publisher='vcp-test';version='0.0.1';engines=@{vscode=$editor.version};activationEvents=@('*');main='./driver.cjs';files=@('driver.cjs');extensionKind=@('workspace');capabilities=@{untrustedWorkspaces=@{supported=$true}}} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $driver 'package.json') -Encoding utf8NoBOM
    $driverArchive=Join-Path $root 'driver.vsix'
    $null=Invoke-BetaProcess $node @((Join-Path $repo 'src/packages/vscode/node_modules/@vscode/vsce/vsce'),'package','--no-dependencies','--allow-missing-repository','--skip-license','--out',$driverArchive) $driver
    $null=Invoke-BetaProcess $Code ($base+@('--install-extension',$driverArchive,'--force')) $root $editorEnvironment
    Copy-Item -LiteralPath (Join-Path $repo 'src/packages/vscode/scripts/run-extension-host.ps1') -Destination $launch
    $progress.inventory=Verify-Payload
} else {
    $progress=Get-Content -LiteralPath $progressPath -Raw | ConvertFrom-Json
    $previous=if ($Mode -ceq 'trusted') { 'restricted' } else { 'trusted' }
    if ($progress.schema -cne 'vcp-editor-refusals-private/1' -or $progress.binding -cne $binding -or $progress.mode -cne $previous) { throw 'Inputs changed or refusal observations out of order' }
    $app=Join-Path $root 'Program Files café'
    $installed=@{app=$app;launcher=(Join-Path $app 'vcp.exe');engine=(Join-Path $app "engine/releases/$($native.archive_sha256)/vcp.exe");data=$DataRoot;registration='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VCP.InternalBeta.1_is1'}
    $null=Verify-Payload
}
if ($Mode -ceq 'finish') {
    # The Rust owner calls finish only AFTER canonical preservation assertions.
    $null=Invoke-BetaProcess $Code ($base+@('--uninstall-extension','iokaio.vcp-local')) $root $editorEnvironment
    $listed=Invoke-BetaProcess $Code ($base+@('--list-extensions','--show-versions')) $root $editorEnvironment
    if (@($listed.stdout -split '\r?\n' | Where-Object { $_ -match '^iokaio\.vcp-local@' }).Count) { throw 'VCP remains installed' }
    Uninstall-BetaCandidate $installed $root
    @{schema='vcp-editor-refusal-observation/1';status='pass';mode='finish';uninstalled=$true} | ConvertTo-Json -Compress
    exit 0
}
$user=Join-Path $root ($Mode+'-user')
$workspaceFile=Join-Path $root ($Mode+'.code-workspace')
foreach ($directory in @((Join-Path $user 'User'),(Join-Path $user 'shared-data/sharedStorage'))) { New-Item -ItemType Directory -Path $directory -Force | Out-Null }
@{'security.workspace.trust.enabled'=$true;'security.workspace.trust.startupPrompt'='never';'security.workspace.trust.emptyWindow'=$false;'update.mode'='none';'extensions.autoUpdate'=$false;'extensions.autoCheckUpdates'=$false;'telemetry.telemetryLevel'='off';'workbench.startupEditor'='none';'files.autoSave'='off';'window.restoreWindows'='none';'editor.formatOnSave'=$false} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $user 'User/settings.json') -Encoding utf8NoBOM
@{folders=@(@{path=$Workspace},@{path=(Join-Path $root 'uninitialized')})} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $workspaceFile -Encoding utf8NoBOM
if ($Mode -ceq 'trusted') { $null=Invoke-BetaProcess $node @($contract,'seed-trust',(Join-Path $user 'shared-data/sharedStorage/state.vscdb'),$Workspace,(Join-Path $root 'uninitialized'),$workspaceFile) $root }
$inputFile=Join-Path $root ($Mode+'-input.json'); $resultFile=Join-Path $root ($Mode+'-result.json')
if (Test-Path -LiteralPath $resultFile) { throw 'Fresh observation result required' }
@{restrictedPath=$true;code=$Code;editorRuntime=$editor.runtime;installed=$true;workspaceFile=$workspaceFile;userData=$user;extensions=$extensions;result=$resultFile;stdout=(Join-Path $root ($Mode+'-stdout.log'));stderr=(Join-Path $root ($Mode+'-stderr.log'));diagnostics=(Join-Path $root ($Mode+'-editor-logs'));runtimeEvidence=(Join-Path $root ($Mode+'-runtime.json'));workspace=$Workspace;data=$DataRoot;executable=$installed.engine;scope=$scopeValue;task=$Task;mode=$Mode;version=$vsix.extension.version;checkout=$repo;uninitialized=(Join-Path $root 'uninitialized');wrongData=(Join-Path $root 'wrong-data');outside=(Join-Path $root 'outside');sourceText="original human work`n";driverSha256=$driverHash} | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $inputFile -Encoding utf8NoBOM
$null=Invoke-BetaProcess $pwsh @('-NoProfile','-File',$launch,'-InputFile',$inputFile) $root @{} 150
$report=(Invoke-BetaProcess $node @($contract,'observation',$inputFile,$resultFile) $root).stdout | ConvertFrom-Json
$null=Verify-Payload
$report | Add-Member -NotePropertyMembers @{artifact_source_commit=$native.manifest.release.reviewed_commit;candidate_id=$native.manifest.release.candidate_id;native_result_sha256=$nativeHash;native_sha256=$native.archive_sha256;setup_sha256=$setup.archive.sha256;vsix_sha256=$vsix.archive.sha256;engine_sha256=(Get-FileHash -LiteralPath $installed.engine).Hash.ToLowerInvariant();native_payload_verified=$true;editor=$editor;installed_inventory_sha256=$progress.inventory;driver_archive_sha256=(Get-FileHash -LiteralPath (Join-Path $root 'driver.vsix')).Hash.ToLowerInvariant()}
$progress.mode=$Mode
$progress | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $progressPath -Encoding utf8NoBOM
$report | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $root ($Mode+'-observation.json')) -Encoding utf8NoBOM
$report | ConvertTo-Json -Depth 16
