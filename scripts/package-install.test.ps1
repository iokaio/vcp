# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-package-install-' + [guid]::NewGuid())
$source = Join-Path $temporary 'source'; $data = Join-Path $temporary 'User Data'; $install = Join-Path $temporary 'Program Files'; $unowned = Join-Path $temporary 'Existing Install'; $invalidInstall = Join-Path $temporary 'Invalid Install'
New-Item -ItemType Directory -Path $source,$data | Out-Null
try {
    $executable = Join-Path $source 'fake.exe'; Set-Content -LiteralPath $executable -Value 'candidate-one'
    foreach ($name in @('workspace.sentinel','history.sentinel','key.sentinel','vault.sentinel')) { Set-Content -LiteralPath (Join-Path $data $name) -Value 'preserve' }
    $resultPath = & pwsh -NoProfile -File (Join-Path $repository 'scripts/package.ps1') -Executable $executable -OutputRoot $temporary | Select-Object -Last 1
    $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    $zip = Join-Path (Split-Path $resultPath) $result.package
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
