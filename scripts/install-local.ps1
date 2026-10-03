#Requires -Version 7.0
# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Open the latest completed unsigned local VCP installer.
.EXAMPLE
.\scripts\install-local.ps1
.EXAMPLE
.\scripts\install-local.ps1 -WhatIf
#>
[CmdletBinding(SupportsShouldProcess)]
param([string]$BuildRoot = (Join-Path $PSScriptRoot '../artifacts/local-setup'))
$ErrorActionPreference = 'Stop'

function Assert-LocalInstallerPath([string]$Path) {
    for ($cursor = [IO.Path]::GetFullPath($Path); $cursor; $cursor = [IO.Path]::GetDirectoryName($cursor)) {
        if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "Linked local build path rejected: $cursor"
        }
    }
}

function Get-LatestLocalInstaller([string]$Root) {
    if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
        throw 'No local installer build found. Run scripts/build-local-setup.ps1 first.'
    }
    Assert-LocalInstallerPath $Root
    $candidates = @(foreach ($file in Get-ChildItem -LiteralPath $Root -Filter 'setup-result.json' -File -Recurse) {
        Assert-LocalInstallerPath $file.FullName
        if ($file.Length -gt 16MB) { throw "Local build record exceeds size limit: $($file.FullName)" }
        $record = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json -Depth 100
        if ($record.schema -cne 'vcp-setup-result/1' -or $record.status -cne 'unsigned-local-candidate') { continue }
        $version = [string]$record.local_candidate.native_version
        if ($record.local_candidate.schema -cne 'vcp-local-candidate/1' -or
            $record.local_candidate.signing -cne 'unsigned' -or
            $version -cnotmatch '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$') {
            throw "Invalid local candidate identity: $($file.FullName)"
        }
        [pscustomobject]@{ Version = [version]$version; Completed = $file.LastWriteTimeUtc; RecordPath = $file.FullName; Record = $record }
    })
    $latest = $candidates | Sort-Object Version, Completed -Descending | Select-Object -First 1
    if (-not $latest) { throw 'No completed unsigned local installer found. Run scripts/build-local-setup.ps1 first.' }
    $archive = $latest.Record.archive
    if ($archive.file -cne "vcp-$($latest.Version)-windows-x64-unsigned-setup.exe" -or
        $archive.sha256 -cnotmatch '^[a-f0-9]{64}$') {
        throw "Invalid installer name or hash: $($latest.RecordPath)"
    }
    $installer = Join-Path (Split-Path -Parent $latest.RecordPath) $archive.file
    if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) { throw "Latest local installer is missing: $installer" }
    Assert-LocalInstallerPath $installer
    if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant() -cne $archive.sha256) {
        throw "Latest local installer does not match its build record: $installer"
    }
    return [pscustomobject]@{ Version = $latest.Version; Path = $installer; RecordPath = $latest.RecordPath }
}

if ($MyInvocation.InvocationName -ne '.') {
    if (-not $IsWindows) { throw 'The local VCP installer requires Windows.' }
    $selected = Get-LatestLocalInstaller $BuildRoot
    Write-Host "Local VCP $($selected.Version): $($selected.Path)"
    if ($PSCmdlet.ShouldProcess($selected.Path, 'Open the local VCP installer')) {
        $installer = Start-Process -FilePath $selected.Path -WindowStyle Normal -PassThru -Wait
        exit $installer.ExitCode
    }
}
