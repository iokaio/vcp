# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PreviousPackage,
    [Parameter(Mandatory)][string]$CandidatePackage,
    [Parameter(Mandatory)][string]$Evidence
)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$installer = Join-Path $repository 'scripts/package-install.ps1'
$previous = (Resolve-Path -LiteralPath $PreviousPackage).Path
$candidate = (Resolve-Path -LiteralPath $CandidatePackage).Path
$oldHash = (Get-FileHash -LiteralPath $previous).Hash.ToLowerInvariant()
$newHash = (Get-FileHash -LiteralPath $candidate).Hash.ToLowerInvariant()
if ($oldHash -eq $newHash) { throw 'Qualification requires distinct package artifacts' }
$temporaryBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\','/')
$owned = Join-Path $temporaryBase ('vcp-editor-upgrade-' + [guid]::NewGuid())
$install = Join-Path $owned 'installation'
$data = Join-Path $owned 'independent-data'
$keys = Join-Path $owned 'independent-keys'
$faultBefore = $env:VCP_PACKAGE_INSTALL_FAULT
$result = [ordered]@{ schema='vcp-editor-native-upgrade/1'; ok=$false; previous_archive_sha256=$oldHash; candidate_archive_sha256=$newHash }
function Invoke-Install([string]$Action, [string]$Archive, [string]$State = '') {
    $arguments = @('-NoProfile','-File',$installer,'-Action',$Action,'-InstallRoot',$install,'-DataRoot',$data)
    if ($Archive) { $arguments += @('-PackageZip',$Archive) }
    if ($State) { $arguments += @('-StateManifest',$State) }
    & (Join-Path $PSHOME 'pwsh.exe') @arguments *> (Join-Path $owned 'last-installer.log')
    return $LASTEXITCODE
}
function Assert-Preserved {
    if ((Get-FileHash -LiteralPath (Join-Path $data 'history.sentinel')).Hash -cne $historyHash -or
        (Get-FileHash -LiteralPath (Join-Path $keys 'recovery.sentinel')).Hash -cne $keyHash) {
        throw 'Installer changed independent data or key material'
    }
}
try {
    New-Item -ItemType Directory -Path $owned,$data,$keys | Out-Null
    [IO.File]::WriteAllText((Join-Path $data 'history.sentinel'), 'Independent history preservation fixture; canonical replay is qualified separately.')
    [IO.File]::WriteAllBytes((Join-Path $keys 'recovery.sentinel'), [Security.Cryptography.RandomNumberGenerator]::GetBytes(64))
    $historyHash = (Get-FileHash -LiteralPath (Join-Path $data 'history.sentinel')).Hash
    $keyHash = (Get-FileHash -LiteralPath (Join-Path $keys 'recovery.sentinel')).Hash
    Remove-Item Env:VCP_PACKAGE_INSTALL_FAULT -ErrorAction SilentlyContinue
    if ((Invoke-Install 'Install' $previous) -ne 0) { throw 'Actual prior native package installation failed' }
    $pointer = Join-Path $install 'active.json'
    if ((Get-Content -LiteralPath $pointer -Raw | ConvertFrom-Json).release -cne $oldHash) { throw 'Initial package identity differs' }
    $beforePointer = [IO.File]::ReadAllBytes($pointer)
    $env:VCP_PACKAGE_INSTALL_FAULT = 'before-activation'
    if ((Invoke-Install 'Upgrade' $candidate) -eq 0) { throw 'Deterministic pre-activation interruption did not occur' }
    if ([Convert]::ToBase64String($beforePointer) -cne [Convert]::ToBase64String([IO.File]::ReadAllBytes($pointer))) { throw 'Interrupted upgrade changed the active pointer' }
    Assert-Preserved
    Remove-Item Env:VCP_PACKAGE_INSTALL_FAULT -ErrorAction SilentlyContinue
    if ((Invoke-Install 'Upgrade' $candidate) -ne 0) { throw 'Retry of interrupted native upgrade failed' }
    $active = Get-Content -LiteralPath $pointer -Raw | ConvertFrom-Json
    if ($active.release -cne $newHash -or $active.previous_release -cne $oldHash) { throw 'Recovered upgrade lost exact release identities' }
    $executable = Join-Path $install ('releases/' + $newHash + '/vcp.exe')
    & $executable --help *> (Join-Path $owned 'native-help.log')
    if ($LASTEXITCODE -ne 0) { throw 'Installed native candidate did not run' }
    $result.executable_sha256 = (Get-FileHash -LiteralPath $executable).Hash.ToLowerInvariant()
    $state = Join-Path $owned 'incompatible-state.json'
    [IO.File]::WriteAllText($state, '{"compatibility":{"canonical":"unsupported-future-store","config":"vcp-cli-profile/1","index":"derived-index-rebuild-required"}}')
    $beforePointer = [IO.File]::ReadAllBytes($pointer)
    if ((Invoke-Install 'Upgrade' $candidate $state) -eq 0) { throw 'Incompatible canonical format was accepted' }
    if ([Convert]::ToBase64String($beforePointer) -cne [Convert]::ToBase64String([IO.File]::ReadAllBytes($pointer))) { throw 'Rejected format changed active release' }
    Assert-Preserved
    if ((Invoke-Install 'Uninstall' '') -ne 0) { throw 'Native package uninstall failed' }
    if (Test-Path -LiteralPath $install) { throw 'Native uninstall left its installation directory' }
    Assert-Preserved
    $result.ok = $true
    $result.interruption = 'deterministic installer fault after staging and before active pointer replacement'
    $result.recovery = 'same candidate retry activated exact retained release'
    $result.incompatible_state_rejected = $true
    $result.data_and_independent_material_preserved = $true
    $result.uninstall = 'installation removed; independent roots retained'
} finally {
    if ($null -eq $faultBefore) { Remove-Item Env:VCP_PACKAGE_INSTALL_FAULT -ErrorAction SilentlyContinue }
    else { $env:VCP_PACKAGE_INSTALL_FAULT = $faultBefore }
    $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath ([IO.Path]::GetFullPath($Evidence)) -Encoding utf8
    $resolved = [IO.Path]::GetFullPath($owned)
    if (-not $resolved.StartsWith($temporaryBase + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        [IO.Path]::GetFileName($resolved) -notmatch '^vcp-editor-upgrade-[a-f0-9-]{36}$') { throw 'Fixture cleanup escaped owned temporary directory' }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
