# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot, [string]$TargetRoot, [string]$Toolchain = 'stable', [ValidateRange(1,16)][int]$Jobs = 4)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if (-not $IsWindows) { Write-Output '{"status":"not_run","reason":"Native Windows required"}'; exit 3 }
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/p7-mcp-numeric-qualification' }
if (-not $TargetRoot) { $TargetRoot = Join-Path $repository 'artifacts/codex-target' }
$directory = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$manifest = Join-Path $directory 'manifest.json'
$record = [ordered]@{
    schema = 'p7-mcp-numeric-qualification/1'; task = 'P7-03'; status = 'running'
    started_at = [DateTime]::UtcNow.ToString('o'); stages = @()
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
    os = [Runtime.InteropServices.RuntimeInformation]::OSDescription
    scope = 'Exact MCP schema profile, model numeric compatibility, canonical wire and captured evidence'
}
function Save-Record {
    $json = $record | ConvertTo-Json -Depth 16
    for ($attempt = 0; ; $attempt++) {
        try {
            [IO.File]::WriteAllText($manifest, $json, [Text.UTF8Encoding]::new($false))
            return
        } catch [IO.IOException] {
            # Concurrent progress readers can briefly hold a Windows sharing lock.
            if ($attempt -ge 4 -or ($_.Exception.HResult -band 0xffff) -notin @(32, 33)) { throw }
            Start-Sleep -Milliseconds 100
        }
    }
}
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
    $common = @("+$Toolchain", 'test', '--manifest-path', (Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.toml'), '--locked', '--offline', '--target-dir', ([IO.Path]::GetFullPath($TargetRoot)), '-j', "$Jobs")
    Stage 'mcp-core' ($common + @('-p', 'vcp-extensions', '--lib', '--test', 'mcp_content', '--test', 'mcp_client', '--test', 'mcp_identity', '--test', 'mcp_schema', '--test', 'mcp_http', '--test', 'mcp_http_session', '--', '--test-threads=1'))
    Stage 'models' ($common + @('-p', 'vcp-models', '--', '--test-threads=1'))
    Stage 'protocol-preservation' ($common + @('-p', 'vcp-protocol', '--features', 'serde_json/arbitrary_precision', '--', '--test-threads=1'))
    Stage 'store-preservation' ($common + @('-p', 'vcp-store', '--features', 'serde_json/arbitrary_precision', '--test', 'persisted_json', '--', '--test-threads=1'))
    Stage 'http-boundaries-and-lifecycle' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--lib', '--', '--test-threads=1'))
    Stage 'mcp-fixture-unit' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--bin', 'vcp-mcp-fixture', '--', '--test-threads=1'))
    Stage 'mcp-fixture-wire' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'mcp_fixture', '--', '--test-threads=1'))
    Stage 'mcp-numeric-host' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'canonical_host', 'mcp_numeric', '--', '--test-threads=1'))
    Stage 'mcp-content-host' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'canonical_host', 'mcp_content', '--', '--test-threads=1'))
    Stage 'mcp-host' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'canonical_host', 'mcp::', '--', '--test-threads=1'))
    Stage 'mcp-http-host' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'canonical_host', 'mcp_http', '--', '--test-threads=1'))
    Stage 'mcp-coding' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'canonical_host', 'mcp_coding::', '--', '--test-threads=1'))
    Stage 'mcp-cli' ($common + @('-p', 'vcp-cli', '--features', 'qualification', '--lib', '--', '--test-threads=1'))
    Stage 'duplex-regression' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'duplex_process', '--', '--test-threads=1'))
    Stage 'duplex-host-regression' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'canonical_host', 'duplex::', '--', '--test-threads=1'))
    Stage 'process-host-regression' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'canonical_host', 'process_broker::', '--', '--test-threads=1'))
    $fixture = Join-Path ([IO.Path]::GetFullPath($TargetRoot)) 'debug/vcp-mcp-fixture.exe'
    $record.fixture_sha256 = (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash.ToLowerInvariant()
    $hostLog = Get-Content -LiteralPath (Join-Path $directory 'mcp-http-host.log') -Raw
    $hostBinaryMatch = [regex]::Match($hostLog, '(?m)^\s*Running .*canonical_host\.rs \((.+\.exe)\)\r?$')
    if (-not $hostBinaryMatch.Success) { throw 'Canonical HTTP test executable identity unavailable' }
    $hostBinary = $hostBinaryMatch.Groups[1].Value
    if (-not [IO.Path]::IsPathRooted($hostBinary)) { $hostBinary = Join-Path $repository $hostBinary }
    $record.canonical_http_fixture_binary_sha256 = (Get-FileHash -LiteralPath $hostBinary -Algorithm SHA256).Hash.ToLowerInvariant()
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
