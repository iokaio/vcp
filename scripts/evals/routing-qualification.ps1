# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot, [string]$TargetRoot, [ValidateRange(1,16)][int]$Jobs = 2)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/p6-routing-qualification' }
if (-not $TargetRoot) { $TargetRoot = Join-Path $repository 'artifacts/codex-target' }
$directory = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$manifest = Join-Path $directory 'manifest.json'
$record = [ordered]@{
    schema = 'p6-routing-qualification-run/1'; started_at = [DateTime]::UtcNow.ToString('o')
    status = 'running'; stages = @(); report = 'routing.json'
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
    os = [Runtime.InteropServices.RuntimeInformation]::OSDescription
    architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    processor = $null; physical_memory_bytes = $null
}
function Save-Record { $record | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $manifest -Encoding utf8 }
function Source-Identity {
    $identity = & node (Join-Path $PSScriptRoot 'routing-source-identity.cjs')
    if ($LASTEXITCODE -ne 0) { throw 'Routing source identity capture failed' }
    return ($identity | ConvertFrom-Json)
}
function Stage([string]$Name, [string[]]$Arguments) {
    $log = Join-Path $directory "$Name.log"
    & cargo @Arguments *> $log
    $code = $LASTEXITCODE
    $record.stages += @{ name = $Name; arguments = $Arguments; exit_code = $code; log = "$Name.log"; sha256 = (Get-FileHash -LiteralPath $log -Algorithm SHA256).Hash.ToLowerInvariant() }
    Save-Record
    if ($code -ne 0) { throw "$Name failed with exit $code; see $log" }
}
Save-Record
try {
    if ($IsWindows) {
        $record.processor = @(Get-CimInstance Win32_Processor | Select-Object -ExpandProperty Name)
        $record.physical_memory_bytes = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
    }
    $record.source_before = Source-Identity
    $record.rustc = (& rustc --version)
    $common = @('--manifest-path', (Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.toml'), '--locked', '--offline', '--target-dir', ([IO.Path]::GetFullPath($TargetRoot)), '-j', "$Jobs", '-p', 'vcp-models', '--example', 'routing_qualification')
    Stage 'contracts' @('test', '--manifest-path', (Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.toml'), '--locked', '--offline', '--target-dir', ([IO.Path]::GetFullPath($TargetRoot)), '-j', "$Jobs", '-p', 'vcp-models', '--test', 'routing', '--test', 'decision', '--test', 'escalation')
    Stage 'comparison' (@('run') + $common + @('--', (Join-Path $directory 'routing.json')))
    $record.report_sha256 = (Get-FileHash -LiteralPath (Join-Path $directory 'routing.json') -Algorithm SHA256).Hash.ToLowerInvariant()
    $record.status = 'passed'
} catch {
    $record.status = 'failed'
    $record.error = $_.Exception.Message
} finally {
    try {
        $record.source_after = Source-Identity
        $record.source_unchanged = $record.Contains('source_before') -and $record.source_before.content_sha256 -eq $record.source_after.content_sha256 -and $record.source_before.commit -eq $record.source_after.commit
        if (-not $record.source_unchanged) {
            $record.status = 'failed'
            $record.source_error = 'Relevant source changed during execution, or initial source identity is unavailable'
        }
    } catch {
        $record.status = 'failed'
        $record.source_error = $_.Exception.Message
    }
    $record.finished_at = [DateTime]::UtcNow.ToString('o')
    Save-Record
}
Write-Output $manifest
if ($record.status -ne 'passed') { exit 1 }
