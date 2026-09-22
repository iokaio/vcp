# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [string[]]$RuntimePath = @(),
    [string]$Assets,
    [string]$ModelManifest,
    [string]$BuildReceipt,
    [string]$OutputRoot
)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $Assets) { $Assets = Join-Path $repository 'src/skills/builtin' }
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/p8-distribution' }
$node = Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1
$binary = Get-Item -LiteralPath ([IO.Path]::GetFullPath($Executable))
if ($binary.PSIsContainer -or ($binary.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $binary.Length -eq 0) { throw 'Explicit ordinary executable required' }
$out = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
$package = Join-Path $out 'package'
New-Item -ItemType Directory -Path $package -Force | Out-Null
Copy-Item -LiteralPath $binary.FullName -Destination (Join-Path $package 'vcp.exe')
foreach ($notice in @('LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md')) {
    Copy-Item -LiteralPath (Join-Path $repository $notice) -Destination (Join-Path $package $notice)
}
New-Item -ItemType Directory -Path (Join-Path $package 'tools') | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'package-install.ps1') -Destination (Join-Path $package 'tools/package-install.ps1')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'package-inventory.cjs') -Destination (Join-Path $package 'tools/package-inventory.cjs')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'package-models.ps1') -Destination (Join-Path $package 'tools/package-models.ps1')
New-Item -ItemType Directory -Path (Join-Path $package 'models') | Out-Null
Copy-Item -LiteralPath (Join-Path $repository 'src/third_party/components/minilm-assets.json') -Destination (Join-Path $package 'models/minilm-assets.json')
if ($RuntimePath.Count) {
    $runtime = Join-Path $package 'runtime'; New-Item -ItemType Directory -Path $runtime | Out-Null
    foreach ($input in $RuntimePath) {
        $item = Get-Item -LiteralPath ([IO.Path]::GetFullPath($input))
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -eq 0) { throw "Runtime input must be an ordinary file: $input" }
        $destination = Join-Path $runtime $item.Name
        if (Test-Path -LiteralPath $destination) { throw "Duplicate runtime filename: $($item.Name)" }
        Copy-Item -LiteralPath $item.FullName -Destination $destination
    }
}
New-Item -ItemType Directory -Path (Join-Path $package 'skills') | Out-Null
$assetJson = & $node.Source (Join-Path $PSScriptRoot 'skills/builtin-assets.cjs') stage ([IO.Path]::GetFullPath($Assets)) (Join-Path $package 'skills/builtin')
if ($LASTEXITCODE -ne 0) { throw 'Builtin asset validation/staging failed' }
$assetInventory = $assetJson | ConvertFrom-Json
$model = [ordered]@{ bundled = $false; specification = 'models/minilm-assets.json'; provisioner = 'tools/package-models.ps1'; acquisition = 'explicit command only'; records = @() }
if ($ModelManifest) {
    $model.records = @(Get-Content -LiteralPath ([IO.Path]::GetFullPath($ModelManifest)) -Raw | ConvertFrom-Json)
    foreach ($record in $model.records) {
        if ($record -is [string] -or $record.digest -notmatch '^[a-f0-9]{64}$' -or -not $record.id) { throw 'Model records require id and SHA-256 digest' }
    }
}
$gitCommitRaw = & git -c safe.directory=$($repository.Replace('\', '/')) -C $repository rev-parse HEAD 2>$null
if ($LASTEXITCODE -ne 0 -or -not $gitCommitRaw) { throw 'Cannot identify the package source Git commit' }
$gitCommit = ([string]$gitCommitRaw).Trim()
if ($gitCommit -notmatch '^[a-f0-9]{40}$') { throw 'Package source Git commit is invalid' }
$gitStatusRaw = & git -c safe.directory=$($repository.Replace('\', '/')) -C $repository status --porcelain=v1 --untracked-files=all 2>$null
if ($LASTEXITCODE -ne 0) { throw 'Cannot identify the package source Git status' }
$dirty = [bool]$gitStatusRaw
$build = [ordered]@{ status = 'caller-supplied-unverified'; executable_sha256 = (Get-FileHash -LiteralPath $binary.FullName).Hash.ToLowerInvariant() }
if ($BuildReceipt) {
    $receiptPath = [IO.Path]::GetFullPath($BuildReceipt)
    $receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
    if ($receipt.schema -ne 'vcp-local-build/1' -or $receipt.exit_code -ne 0 -or $receipt.executable_sha256 -cne $build.executable_sha256) { throw 'Build receipt does not bind this executable' }
    Copy-Item -LiteralPath $receiptPath -Destination (Join-Path $package 'build-receipt.json')
    $build = [ordered]@{ status = 'recorded-local-build'; receipt = 'build-receipt.json'; receipt_sha256 = (Get-FileHash -LiteralPath $receiptPath).Hash.ToLowerInvariant() }
}
$metadata = [ordered]@{
    source = [ordered]@{ repository = 'vcp'; git_commit = if ($gitCommit -match '^[a-f0-9]{40}$') { $gitCommit } else { $null }; dirty = $dirty }
    target = [ordered]@{ os = 'Windows'; architecture = $env:PROCESSOR_ARCHITECTURE }
    build = $build
    compatibility = [ordered]@{ cli = 'vcp-cli/0.1.0'; worker = 'in-process'; canonical = 'vcp-store/1+replay-base/2'; config = 'vcp-cli-profile/1'; index = 'derived-index-rebuild-required' }
    runtime = @($RuntimePath | ForEach-Object { [IO.Path]::GetFileName($_) })
    skills = [ordered]@{ catalog_sha256 = $assetInventory.catalog_sha256; catalog_version = $assetInventory.version; skills = $assetInventory.skills }
    model_provisioning = $model
}
$inventoryTool = Join-Path $PSScriptRoot 'package-inventory.cjs'
$metadataPath = Join-Path $out 'metadata.json'; $metadata | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $metadataPath -Encoding utf8
Push-Location $repository
try {
    & $node.Source -e "const fs=require('node:fs'), p=require('./scripts/package-inventory.cjs'); const m=p.buildManifest(process.argv[1], JSON.parse(fs.readFileSync(process.argv[2],'utf8'))); fs.writeFileSync(require('node:path').join(process.argv[1],'manifest.json'), JSON.stringify(m,null,2)+'\n'); p.verifyManifest(process.argv[1],m);" $package $metadataPath
} finally { Pop-Location }
if ($LASTEXITCODE -ne 0) { throw 'Distribution manifest validation failed' }
$archive = Join-Path $out 'vcp-windows-unsigned.zip'
[IO.Compression.ZipFile]::CreateFromDirectory($package, $archive, [IO.Compression.CompressionLevel]::Optimal, $false)
$manifest = Get-Content -LiteralPath (Join-Path $package 'manifest.json') -Raw | ConvertFrom-Json
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try { $entries = @($zip.Entries | ForEach-Object FullName) } finally { $zip.Dispose() }
$expectedEntries = @($manifest.files.path + 'manifest.json') | Sort-Object
if ((ConvertTo-Json @($entries | Sort-Object) -Compress) -cne (ConvertTo-Json @($expectedEntries) -Compress)) { throw 'ZIP entries differ from the manifest inventory' }
$result = [ordered]@{ schema = 'vcp-distribution-result/1'; status = 'candidate'; package = 'vcp-windows-unsigned.zip'; archive_sha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant(); manifest = $manifest; entries = $entries; limitations = $manifest.limitations }
$result | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $out 'result.json') -Encoding utf8
Write-Output (Join-Path $out 'result.json')
