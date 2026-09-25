# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$PreviousArchive,
    [string]$OutputRoot
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Actual native Windows is required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/cs-authoring-package' }
$candidate = [IO.Path]::GetFullPath($Executable)
$previous = [IO.Path]::GetFullPath($PreviousArchive)
# The installer owns only a new disposable installation outside the checkout.
# No recursive cleanup is done here; retain evidence and failed installations.
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-authoring-install-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $temporary | Out-Null
$resolved = [IO.Path]::GetFullPath($temporary)
if ($resolved.StartsWith($repository.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Installation must be outside checkout' }
$install = Join-Path $resolved 'installation'
$data = Join-Path $resolved 'protected-data'
New-Item -ItemType Directory -Path $data | Out-Null
$sentinel = Join-Path $data 'user-state.txt'
[IO.File]::WriteAllText($sentinel, 'original synthetic user state')
$sentinelHash = (Get-FileHash -LiteralPath $sentinel).Hash
$record = [ordered]@{
    schema = 'cs-authoring-package-qualification/1'; task = 'CS-1'
    status = 'running'; directory = $resolved; stages = @()
    executable_sha256 = (Get-FileHash -LiteralPath $candidate).Hash.ToLowerInvariant()
    previous_archive_sha256 = (Get-FileHash -LiteralPath $previous).Hash.ToLowerInvariant()
    limitations = @('Local candidate build, not release/performance qualification.', 'Synthetic protected-data sentinel; canonical migration and live usefulness are separate gates.')
}
$report = Join-Path $resolved 'result.json'
function Save-Report { $record | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $report -Encoding utf8 }
function Active-Release {
    $pointer = Get-Content -LiteralPath (Join-Path $install 'active.json') -Raw | ConvertFrom-Json
    if ($pointer.release -cnotmatch '^[a-f0-9]{64}$') { throw 'Invalid installed release identity' }
    return Join-Path (Join-Path $install 'releases') $pointer.release
}
function Install-Step([string]$Action, [string]$Archive) {
    $arguments = @('-NoProfile', '-File', (Join-Path $repository 'scripts/package-install.ps1'), '-Action', $Action, '-InstallRoot', $install, '-DataRoot', $data)
    if ($Archive) { $arguments += @('-PackageZip', $Archive) }
    & pwsh @arguments *> (Join-Path $resolved ($Action + '-' + $record.stages.Count + '.log'))
    if ($LASTEXITCODE -ne 0) { throw "$Action failed" }
    if ((Get-FileHash -LiteralPath $sentinel).Hash -cne $sentinelHash) { throw 'Protected data changed' }
    $release = Active-Release
    $binary = Join-Path $release 'vcp.exe'
    & $binary --help *> (Join-Path $resolved ('help-' + $record.stages.Count + '.log'))
    if ($LASTEXITCODE -ne 0) { throw 'Installed native executable failed startup' }
    $catalog = Join-Path $release 'skills/builtin/catalog.json'
    $metadata = Get-Content -LiteralPath $catalog -Raw | ConvertFrom-Json
    $record.stages += @{ action = $Action; release = $release; executable_sha256 = (Get-FileHash -LiteralPath $binary).Hash.ToLowerInvariant(); catalog_sha256 = (Get-FileHash -LiteralPath $catalog).Hash.ToLowerInvariant(); skills = $metadata.skills.Count; version = $metadata.version; protected_data_unchanged = $true }
    Save-Report
}
Save-Report
try {
    $resultPath = & pwsh -NoProfile -File (Join-Path $repository 'scripts/package.ps1') -Executable $candidate -OutputRoot $OutputRoot | Select-Object -Last 1
    if ($LASTEXITCODE -ne 0) { throw 'Candidate packaging failed' }
    $package = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    $archive = Join-Path (Split-Path -Parent $resultPath) $package.package
    $record.package_result = $resultPath
    $record.archive_sha256 = $package.archive_sha256
    Install-Step 'Install' $previous
    Install-Step 'Upgrade' $archive
    if ($record.stages[1].executable_sha256 -cne $record.executable_sha256 -or $record.stages[1].catalog_sha256 -cne $package.manifest.skills.catalog_sha256) { throw 'Installed candidate identity differs' }
    Install-Step 'Rollback' ''
    if ($record.stages[2].release -cne $record.stages[0].release -or $record.stages[2].catalog_sha256 -cne $record.stages[0].catalog_sha256 -or $record.stages[2].executable_sha256 -cne $record.stages[0].executable_sha256) { throw 'Rollback did not restore exact prior binary/catalog' }
    Install-Step 'Upgrade' $archive
    if ($record.stages[3].release -cne $record.stages[1].release) { throw 'Re-upgrade changed candidate identity' }
    $record.status = 'pass'
    $record.installed_candidate = Active-Release
} catch {
    $record.status = 'fail'
    $record.error = $_.Exception.Message
} finally { Save-Report }
Write-Output $report
if ($record.status -ne 'pass') { exit 1 }
