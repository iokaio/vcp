# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot, [string]$TargetRoot, [string]$Toolchain = 'stable', [ValidateRange(1,16)][int]$Jobs = 4)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if (-not $IsWindows) { Write-Output '{"status":"not_run","reason":"Native Windows required"}'; exit 3 }
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/p7-duplex-qualification' }
if (-not $TargetRoot) { $TargetRoot = Join-Path $repository 'artifacts/codex-target' }
$directory = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$manifest = Join-Path $directory 'manifest.json'
$record = [ordered]@{
    schema = 'p7-duplex-qualification/1'; task = 'P7-03'; status = 'running'
    started_at = [DateTime]::UtcNow.ToString('o'); stages = @()
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
    os = [Runtime.InteropServices.RuntimeInformation]::OSDescription
    scope = 'Owned duplex process and canonical broker prerequisite; no MCP protocol or remote service qualification'
}
function Save-Record { $record | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath $manifest -Encoding utf8 }
function Source-Identity {
    $identity = & node (Join-Path $PSScriptRoot 'memory-source-identity.cjs')
    if ($LASTEXITCODE -ne 0) { throw 'Source identity capture failed' }
    return ($identity | ConvertFrom-Json)
}
function Stage([string]$Name, [string[]]$Arguments) {
    $log = Join-Path $directory "$Name.log"
    & cargo @Arguments *> $log
    $code = $LASTEXITCODE
    $contents = Get-Content -LiteralPath $log -Raw
    $tests = @([regex]::Matches($contents, '(?m)^test ([^\r\n]+) \.\.\. ok\r?$') | ForEach-Object { $_.Groups[1].Value })
    $record.stages += @{ name = $Name; arguments = $Arguments; exit_code = $code; tests = $tests; log = "$Name.log"; sha256 = (Get-FileHash -LiteralPath $log -Algorithm SHA256).Hash.ToLowerInvariant() }
    Save-Record
    if ($code -ne 0 -or $tests.Count -eq 0 -or $contents -match 'panicked at') { throw "$Name failed, selected no tests, or had a background panic; see $log" }
}
Save-Record
try {
    $record.source_before = Source-Identity
    $record.rustc = (& rustc "+$Toolchain" --version)
    if ($LASTEXITCODE -ne 0) { throw 'Requested Rust toolchain unavailable' }
    $record.msvc = $env:VCToolsVersion
    $common = @("+$Toolchain", 'test', '--manifest-path', (Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.toml'), '--locked', '--offline', '--target-dir', ([IO.Path]::GetFullPath($TargetRoot)), '-j', "$Jobs", '-p', 'vcp-lifecycle', '--features', 'qualification')
    Stage 'duplex-process' ($common + @('--test', 'duplex_process', '--', '--test-threads=1'))
    Stage 'duplex-host' ($common + @('--test', 'canonical_host', 'duplex::', '--', '--test-threads=1'))
    Stage 'existing-process-broker' ($common + @('--test', 'canonical_host', 'process_broker::', '--', '--test-threads=1'))
    $fixture = Join-Path ([IO.Path]::GetFullPath($TargetRoot)) 'debug/vcp-process-fixture.exe'
    $record.fixture_sha256 = (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash.ToLowerInvariant()
    $record.status = 'passed'
} catch {
    $record.status = 'failed'
    $record.error = $_.Exception.Message
} finally {
    try {
        $record.source_after = Source-Identity
        $record.source_unchanged = $record.Contains('source_before') -and $record.source_before.content_sha256 -eq $record.source_after.content_sha256 -and $record.source_before.commit -eq $record.source_after.commit
        if (-not $record.source_unchanged) { $record.status = 'failed'; $record.source_error = 'Relevant source changed during execution' }
    } catch { $record.status = 'failed'; $record.source_error = $_.Exception.Message }
    $record.finished_at = [DateTime]::UtcNow.ToString('o')
    Save-Record
}
Write-Output $manifest
if ($record.status -ne 'passed') { exit 1 }
