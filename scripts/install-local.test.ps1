#Requires -Version 7.0
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'install-local.ps1')
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testRoot = Join-Path $tempBase ('vcp-local-installer-test-' + [guid]::NewGuid().ToString('N'))
function Assert-Test([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Add-Fixture([string]$Version, [string]$Status = 'unsigned-local-candidate') {
    $directory = Join-Path $testRoot "$Version/build"
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $file = "vcp-$Version-windows-x64-unsigned-setup.exe"
    $path = Join-Path $directory $file
    # Inert fixture bytes: selection is exercised without starting any installer.
    Set-Content -LiteralPath $path -Value "fixture $Version" -NoNewline
    $record = @{
        schema = 'vcp-setup-result/1'; status = $Status
        local_candidate = @{ schema = 'vcp-local-candidate/1'; native_version = $Version; signing = 'unsigned' }
        archive = @{ file = $file; sha256 = (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() }
    }
    $record | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $directory 'setup-result.json')
    return $path
}
try {
    $missingRejected = $false
    try { Get-LatestLocalInstaller $testRoot | Out-Null } catch { $missingRejected = $_ -like '*No local installer build*' }
    Assert-Test $missingRejected 'Missing builds were not explained'
    $old = Add-Fixture '0.2.9'
    $new = Add-Fixture '0.2.10'
    [void](Add-Fixture '0.2.11' 'building')
    (Get-Item -LiteralPath (Join-Path (Split-Path -Parent $old) 'setup-result.json')).LastWriteTimeUtc = [DateTime]::UtcNow.AddDays(1)
    Assert-Test ((Get-LatestLocalInstaller $testRoot).Path -eq $new) 'Latest completed numeric version was not selected'
    & (Join-Path $PSScriptRoot 'install-local.ps1') -BuildRoot $testRoot -WhatIf
    Add-Content -LiteralPath $new -Value 'modified'
    $hashRejected = $false
    try { Get-LatestLocalInstaller $testRoot | Out-Null } catch { $hashRejected = $_ -like '*does not match its build record*' }
    Assert-Test $hashRejected 'Modified latest installer silently fell back to an older build'
    $recordPath = Join-Path (Split-Path -Parent $new) 'setup-result.json'
    $record = Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json
    $record.archive.file = '../other.exe'
    $record | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $recordPath
    $pathRejected = $false
    try { Get-LatestLocalInstaller $testRoot | Out-Null } catch { $pathRejected = $_ -like '*Invalid installer name*' }
    Assert-Test $pathRejected 'Installer path traversal was accepted'
    Write-Host 'Local installer selection passed: missing builds, numeric ordering, completed-only, WhatIf, hash mismatch and unsafe path.'
}
finally {
    $resolved = [IO.Path]::GetFullPath($testRoot)
    if (-not $resolved.StartsWith($tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        (Split-Path -Leaf $resolved) -notlike 'vcp-local-installer-test-*') { throw 'Unsafe test cleanup path' }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
