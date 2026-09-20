# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [string]$Assets,
    [string]$OutputRoot
)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $Assets) { $Assets = Join-Path $repository 'src/skills/builtin' }
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/p7-builtin-packages' }
Import-Module (Join-Path $PSScriptRoot 'skills/archive.psm1') -Force
$binary = Get-Item -LiteralPath ([IO.Path]::GetFullPath($Executable))
if ($binary.PSIsContainer -or ($binary.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
    $binary.Length -eq 0 -or $binary.Length -gt 1073741824) { throw 'Explicit ordinary executable within 1 GiB required' }
$binaryHash = (Get-FileHash -LiteralPath $binary.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$directory = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
$package = Join-Path $directory 'package'
New-Item -ItemType Directory -Path (Join-Path $package 'skills') -Force | Out-Null
$assetInventoryJson = & node (Join-Path $PSScriptRoot 'skills/builtin-assets.cjs') stage ([IO.Path]::GetFullPath($Assets)) (Join-Path $package 'skills/builtin')
if ($LASTEXITCODE -ne 0) { throw 'Builtin asset validation/staging failed' }
$assetInventory = $assetInventoryJson | ConvertFrom-Json
Copy-Item -LiteralPath $binary.FullName -Destination (Join-Path $package 'vcp.exe')
if ((Get-FileHash -LiteralPath (Join-Path $package 'vcp.exe') -Algorithm SHA256).Hash.ToLowerInvariant() -cne $binaryHash) {
    throw 'Executable changed during staging'
}
foreach ($notice in @('LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md')) {
    Copy-Item -LiteralPath (Join-Path $repository $notice) -Destination (Join-Path $package $notice)
}
$files = @($assetInventory.files | ForEach-Object {
    [ordered]@{ path = "skills/builtin/$($_.path)"; bytes = $_.bytes; sha256 = $_.sha256 }
})
foreach ($relative in @('vcp.exe', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md')) {
    $file = Join-Path $package $relative
    $files += [ordered]@{ path = $relative; bytes = (Get-Item -LiteralPath $file).Length; sha256 = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() }
}
$inventory = [ordered]@{
    schema = 'vcp-skill-package-qualification/1'; created_at = [DateTime]::UtcNow.ToString('o')
    scope = 'Executable and builtin skill assets; not full P8 distribution/install qualification or publication'
    executable_sha256 = $binaryHash; catalog_sha256 = $assetInventory.catalog_sha256
    catalog_version = $assetInventory.version; skills = $assetInventory.skills
    files = @($files | Sort-Object path)
}
$inventory | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $directory 'inventory.json') -Encoding utf8
$archive = Join-Path $directory 'bundle.zip'
[IO.Compression.ZipFile]::CreateFromDirectory($package, $archive, [IO.Compression.CompressionLevel]::Fastest, $false)
$verified = Test-VcpSkillArchive -ArchivePath $archive -Inventory $inventory
$result = [ordered]@{
    schema = 'vcp-skill-package-result/1'; status = $verified.status; entries = $verified.entries
    inventory = 'inventory.json'; package = 'package'; archive = 'bundle.zip'
    archive_sha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
}
$result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $directory 'result.json') -Encoding utf8
Write-Output (Join-Path $directory 'result.json')
