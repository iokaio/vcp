# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-package-install-' + [guid]::NewGuid())
$source = Join-Path $temporary 'source'; $data = Join-Path $temporary 'User Data β'; $install = Join-Path $temporary 'Program Files β'; $unowned = Join-Path $temporary 'Existing Install'; $invalidInstall = Join-Path $temporary 'Invalid Install'
New-Item -ItemType Directory -Path $source,$data | Out-Null
try {
    $executable = Join-Path $source 'fake.exe'; Set-Content -LiteralPath $executable -Value 'candidate-one'
    foreach ($name in @('workspace.sentinel','history.sentinel','key.sentinel','vault.sentinel')) { Set-Content -LiteralPath (Join-Path $data $name) -Value 'preserve' }
    $resultPath = & pwsh -NoProfile -File (Join-Path $repository 'scripts/package.ps1') -Executable $executable -OutputRoot $temporary | Select-Object -Last 1
    $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    $zip = Join-Path (Split-Path $resultPath) $result.package
    # A real cross-process mutex holder excludes every lifecycle action before
    # it creates a root, stages a payload or changes protected data.
    $mutexName = 'Global\VCP.Install.' + [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $operation = [Threading.Mutex]::new($false, $mutexName)
    $operationHeld = $false
    try {
        $operationHeld = $operation.WaitOne(0)
        if (-not $operationHeld) { throw 'Concurrent installer test requires an idle installation lifecycle' }
        foreach ($competingAction in @('Install', 'Upgrade', 'Rollback', 'Uninstall')) {
            $diagnostic = & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action $competingAction -PackageZip $zip -InstallRoot $install -DataRoot $data 2>&1 | Out-String
            if ($LASTEXITCODE -eq 0 -or $diagnostic -notmatch 'Another VCP installation operation' -or (Test-Path -LiteralPath $install)) { throw "Concurrent $competingAction did not refuse before mutation" }
        }
    } finally {
        if ($operationHeld) { $operation.ReleaseMutex() }
        $operation.Dispose()
    }
    # Keep a handle alive while terminating only our own holder process. The
    # next installer sees an abandoned mutex and still performs normal recovery.
    $holderScript = Join-Path $temporary 'mutex-holder.ps1'
    @'
param([string]$Name, [string]$Ready)
$mutex = [Threading.Mutex]::new($false, $Name)
if (-not $mutex.WaitOne(0)) { exit 3 }
[IO.File]::WriteAllText($Ready, 'ready')
[Console]::In.ReadLine() | Out-Null
$mutex.ReleaseMutex()
$mutex.Dispose()
'@ | Set-Content -LiteralPath $holderScript -Encoding utf8NoBOM
    $ready = Join-Path $temporary 'mutex-ready'
    $retainedMutex = [Threading.Mutex]::new($false, $mutexName)
    $start = [Diagnostics.ProcessStartInfo]::new((Get-Command pwsh -CommandType Application).Source)
    $start.UseShellExecute = $false; $start.CreateNoWindow = $true; $start.RedirectStandardInput = $true
    foreach ($argument in @('-NoProfile', '-File', $holderScript, '-Name', $mutexName, '-Ready', $ready)) { $start.ArgumentList.Add($argument) }
    $holder = [Diagnostics.Process]::Start($start)
    try {
        $deadline = [Diagnostics.Stopwatch]::StartNew()
        while (-not (Test-Path -LiteralPath $ready)) {
            if ($holder.HasExited -or $deadline.ElapsedMilliseconds -gt 15000) { throw 'Installer mutex holder did not become ready' }
            Start-Sleep -Milliseconds 20
        }
        $holder.Kill(); $holder.WaitForExit()
        & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Install -PackageZip $zip -InstallRoot $install -DataRoot $data
        if ($LASTEXITCODE -ne 0) { throw 'First install failed after a terminated operation owner' }
        & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Uninstall -InstallRoot $install -DataRoot $data
        if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $install)) { throw 'Abandoned-owner fixture uninstall failed' }
    } finally {
        if (-not $holder.HasExited) { $holder.Kill(); $holder.WaitForExit() }
        $holder.Dispose(); $retainedMutex.Dispose()
    }
    $invalidZip = Join-Path $temporary 'invalid.zip'; [IO.File]::WriteAllText($invalidZip, 'not a ZIP archive')
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Install -PackageZip $invalidZip -InstallRoot $invalidInstall -DataRoot $data
    if ($LASTEXITCODE -eq 0 -or (Test-Path -LiteralPath $invalidInstall)) { throw 'Failed fresh install left an unrecoverable install root' }
    $env:VCP_PACKAGE_INSTALL_FAULT = 'before-activation'
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Install -PackageZip $zip -InstallRoot $install -DataRoot $data
    if ($LASTEXITCODE -eq 0 -or -not (Test-Path -LiteralPath (Join-Path $install '.vcp-install-owned.json')) -or (Test-Path -LiteralPath (Join-Path $install 'active.json'))) { throw 'First-install fault did not retain a bounded recoverable installation' }
    Remove-Item Env:VCP_PACKAGE_INSTALL_FAULT -ErrorAction SilentlyContinue
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Install -PackageZip $zip -InstallRoot $install -DataRoot (Join-Path $temporary 'Wrong Data')
    if ($LASTEXITCODE -eq 0 -or (Test-Path -LiteralPath (Join-Path $install 'active.json'))) { throw 'Install recovery accepted a different protected data root' }
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Install -PackageZip $zip -InstallRoot $install -DataRoot $data
    if ($LASTEXITCODE -ne 0) { throw 'First-install retry did not recover the validated retained release' }
    $active = Get-Content -LiteralPath (Join-Path $install 'active.json') -Raw | ConvertFrom-Json
    if (-not (Test-Path -LiteralPath (Join-Path $install ('releases/' + $active.release + '/vcp.exe')))) { throw 'Active payload missing' }
    New-Item -ItemType Directory -Path $unowned | Out-Null; Set-Content -LiteralPath (Join-Path $unowned 'unrelated.txt') -Value 'keep'
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Install -PackageZip $zip -InstallRoot $unowned -DataRoot $data
    if ($LASTEXITCODE -eq 0) { throw 'Unowned preexisting install directory was accepted' }
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Uninstall -InstallRoot $unowned -DataRoot $data
    if ($LASTEXITCODE -eq 0 -or -not (Test-Path -LiteralPath (Join-Path $unowned 'unrelated.txt'))) { throw 'Unmarked uninstall was not refused safely' }
    Remove-Item -LiteralPath $unowned -Recurse -Force
    $incompatible = Join-Path $temporary 'state.json'
    '{"compatibility":{"canonical":"newer-format","config":"vcp-config-unspecified","index":"vcp-index-unspecified"}}' | Set-Content -LiteralPath $incompatible
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Upgrade -PackageZip $zip -InstallRoot $install -DataRoot $data -StateManifest $incompatible
    $failed = $LASTEXITCODE -ne 0
    if (-not $failed) { throw 'Incompatible state was accepted' }
    Set-Content -LiteralPath $executable -Value 'candidate-two'
    $resultPath2 = & pwsh -NoProfile -File (Join-Path $repository 'scripts/package.ps1') -Executable $executable -OutputRoot $temporary | Select-Object -Last 1
    $result2 = Get-Content -LiteralPath $resultPath2 -Raw | ConvertFrom-Json; $zip2 = Join-Path (Split-Path $resultPath2) $result2.package
    $oldRelease = $active.release
    # Actual on-disk format evidence is required even when StateManifest is
    # omitted. Synthetic headers here test refusal ordering; real-store upgrade
    # qualification remains a separate installed-product gate.
    $row = Join-Path $data 'workspaces/state-probe'
    $canonical = Join-Path $row 'canonical'
    New-Item -ItemType Directory -Path $canonical -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $canonical 'owner.lock'), 'fixture-owner')
    $beforeStateProbe = [IO.File]::ReadAllBytes((Join-Path $install 'active.json'))
    foreach ($backend in @('files', 'sqlite')) {
        @{version=1;config=@{backend=$backend;canonical_root=('\\?\' + $canonical)}} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $row 'workspace.json')
        $formatPath = Join-Path $canonical 'format.json'
        if (Test-Path -LiteralPath $formatPath) { Remove-Item -LiteralPath $formatPath }
        & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Upgrade -PackageZip $zip2 -InstallRoot $install -DataRoot $data
        if ($LASTEXITCODE -eq 0 -or
            [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $install 'active.json'))) -cne [Convert]::ToBase64String($beforeStateProbe)) { throw 'Missing registered format marker was accepted' }
        @{version=999;backend=$backend} | ConvertTo-Json | Set-Content -LiteralPath $formatPath
        $formatHash = (Get-FileHash -LiteralPath $formatPath).Hash
        & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Upgrade -PackageZip $zip2 -InstallRoot $install -DataRoot $data
        if ($LASTEXITCODE -eq 0 -or (Get-FileHash -LiteralPath $formatPath).Hash -cne $formatHash -or
            [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $install 'active.json'))) -cne [Convert]::ToBase64String($beforeStateProbe)) { throw 'Unsupported actual store version was accepted or modified' }
        @{version=1;backend=$backend} | ConvertTo-Json | Set-Content -LiteralPath $formatPath
        $payloadPath = Join-Path $canonical $(if ($backend -eq 'sqlite') { 'canonical.sqlite' } else { 'canonical.frames' })
        [IO.File]::WriteAllText($payloadPath, 'unsupported newer native bytes')
        & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Upgrade -PackageZip $zip2 -InstallRoot $install -DataRoot $data
        if ($LASTEXITCODE -eq 0 -or [IO.File]::ReadAllText($payloadPath) -cne 'unsupported newer native bytes') { throw 'Unsupported actual backend header was accepted or modified' }
        if ($backend -eq 'sqlite') {
            $header = [byte[]]::new(100)
            [Text.Encoding]::ASCII.GetBytes("SQLite format 3`0").CopyTo($header, 0)
            $header[63] = 1
            [IO.File]::WriteAllBytes($payloadPath, $header)
            $wal = $payloadPath + '-wal'
            [IO.File]::WriteAllText($wal, 'uncheckpointed state fixture')
            & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Upgrade -PackageZip $zip2 -InstallRoot $install -DataRoot $data
            if ($LASTEXITCODE -eq 0 -or [IO.File]::ReadAllText($wal) -cne 'uncheckpointed state fixture' -or
                [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $install 'active.json'))) -cne [Convert]::ToBase64String($beforeStateProbe)) { throw 'Retained SQLite WAL was accepted or modified' }
            Remove-Item -LiteralPath $wal
        } else {
            [IO.File]::WriteAllText($payloadPath, 'VCPJ0001')
        }
        $validPayloadHash = (Get-FileHash -LiteralPath $payloadPath).Hash
        & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Upgrade -PackageZip $zip2 -InstallRoot $install -DataRoot $data
        if ($LASTEXITCODE -ne 0) { throw 'Known synthetic canonical header and verbatim local path were refused' }
        & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Rollback -InstallRoot $install -DataRoot $data
        if ($LASTEXITCODE -ne 0 -or (Get-FileHash -LiteralPath $payloadPath).Hash -cne $validPayloadHash) { throw 'Compatible synthetic header rollback failed or changed retained bytes' }
        $beforeStateProbe = [IO.File]::ReadAllBytes((Join-Path $install 'active.json'))
    }
    $activeOwner = [IO.File]::Open((Join-Path $canonical 'owner.lock'), [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::ReadWrite)
    try {
        & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Upgrade -PackageZip $zip2 -InstallRoot $install -DataRoot $data
        if ($LASTEXITCODE -eq 0) { throw 'Upgrade proceeded while canonical state had an active owner' }
    } finally { $activeOwner.Dispose() }
    $resolvedRow = [IO.Path]::GetFullPath($row)
    if (-not $resolvedRow.StartsWith([IO.Path]::GetFullPath($temporary).TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'State fixture escaped its temporary root' }
    Remove-Item -LiteralPath $resolvedRow -Recurse -Force
    $ownedPath = Join-Path $install '.vcp-install-owned.json'
    $ownedBytes = [IO.File]::ReadAllBytes($ownedPath)
    $wrongOwner = Get-Content -LiteralPath $ownedPath -Raw | ConvertFrom-Json
    $wrongOwner.data_root = Join-Path $temporary 'different-data'
    $wrongOwner | ConvertTo-Json | Set-Content -LiteralPath $ownedPath
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Upgrade -PackageZip $zip2 -InstallRoot $install -DataRoot $data
    if ($LASTEXITCODE -eq 0) { throw 'Mismatched ownership marker was accepted' }
    [IO.File]::WriteAllBytes($ownedPath, $ownedBytes)
    $env:VCP_PACKAGE_INSTALL_FAULT = 'before-activation'
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Upgrade -PackageZip $zip2 -InstallRoot $install -DataRoot $data
    if ($LASTEXITCODE -eq 0) { throw 'Fault hook did not interrupt activation' }
    $afterFault = Get-Content -LiteralPath (Join-Path $install 'active.json') -Raw | ConvertFrom-Json
    if ($afterFault.release -ne $oldRelease) { throw 'Fault changed active release before pointer replacement' }
    Remove-Item Env:VCP_PACKAGE_INSTALL_FAULT -ErrorAction SilentlyContinue
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Upgrade -PackageZip $zip2 -InstallRoot $install -DataRoot $data
    $upgraded = Get-Content -LiteralPath (Join-Path $install 'active.json') -Raw | ConvertFrom-Json
    if ($upgraded.release -eq $oldRelease) { throw 'Recovery did not activate staged release' }
    $activePath = Join-Path $install 'active.json'
    $activeBytes = [IO.File]::ReadAllBytes($activePath)
    $upgraded.previous_release = '../../outside'
    $upgraded | ConvertTo-Json | Set-Content -LiteralPath $activePath
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Rollback -InstallRoot $install -DataRoot $data
    if ($LASTEXITCODE -eq 0) { throw 'Unsafe rollback path was accepted' }
    [IO.File]::WriteAllBytes($activePath, $activeBytes)
    $lockedPath = Join-Path $install ('releases/' + $oldRelease + '/vcp.exe')
    $lock = [IO.File]::Open($lockedPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Uninstall -InstallRoot $install -DataRoot $data
        if ($LASTEXITCODE -eq 0 -or -not (Test-Path -LiteralPath $ownedPath)) { throw 'Locked uninstall did not preserve its recovery marker' }
        & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Rollback -InstallRoot $install -DataRoot $data
        if ($LASTEXITCODE -ne 0) { throw 'Compatible rollback failed while old payload was open' }
    } finally { $lock.Dispose() }
    $rolledBack = Get-Content -LiteralPath (Join-Path $install 'active.json') -Raw | ConvertFrom-Json
    if ($rolledBack.release -ne $oldRelease) { throw 'Rollback selected the wrong release' }
    $linked = Join-Path $install 'releases/linked-data'
    New-Item -ItemType Junction -Path $linked -Target $data | Out-Null
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Uninstall -InstallRoot $install -DataRoot $data
    if ($LASTEXITCODE -eq 0 -or -not (Test-Path -LiteralPath (Join-Path $data 'history.sentinel'))) { throw 'Linked installation was not refused safely' }
    Remove-Item -LiteralPath $linked -Force
    & pwsh -NoProfile -File (Join-Path $repository 'scripts/package-install.ps1') -Action Uninstall -InstallRoot $install -DataRoot $data
    foreach ($name in @('workspace.sentinel','history.sentinel','key.sentinel','vault.sentinel')) { if (-not (Test-Path -LiteralPath (Join-Path $data $name))) { throw "Uninstall removed protected data: $name" } }
    Write-Output 'package install integration passed'
} finally {
    Remove-Item Env:VCP_PACKAGE_INSTALL_FAULT -ErrorAction SilentlyContinue
    $resolvedTemporary = [IO.Path]::GetFullPath($temporary)
    if (-not $resolvedTemporary.StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()), [StringComparison]::OrdinalIgnoreCase)) { throw 'Fixture cleanup escaped temporary root' }
    Remove-Item -LiteralPath $resolvedTemporary -Recurse -Force -ErrorAction SilentlyContinue
}
