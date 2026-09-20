# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot, [string]$TargetRoot, [string]$Toolchain = 'stable', [ValidateRange(1,16)][int]$Jobs = 4)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if (-not $IsWindows) { Write-Output '{"status":"not_run","reason":"Native Windows required"}'; exit 3 }
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/p1-persisted-json-qualification' }
if (-not $TargetRoot) { $TargetRoot = Join-Path $repository 'artifacts/codex-target' }
$directory = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$manifest = Join-Path $directory 'manifest.json'
$record = [ordered]@{
    schema = 'p1-persisted-json-qualification/1'; task = 'P1-04'; status = 'running'
    started_at = [DateTime]::UtcNow.ToString('o'); stages = @()
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
    os = [Runtime.InteropServices.RuntimeInformation]::OSDescription
    scope = 'Literal-preserving canonical JSON decoding, storage replay and history consumers'
}
function Save-Record {
    $json = $record | ConvertTo-Json -Depth 16
    for ($attempt = 0; ; $attempt++) {
        try {
            [IO.File]::WriteAllText($manifest, $json, [Text.UTF8Encoding]::new($false))
            return
        } catch [IO.IOException] {
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
    $ignored = @([regex]::Matches($contents, '(?m)^test ([^\r\n]+) \.\.\. ignored[^\r\n]*\r?$') | ForEach-Object { $_.Groups[1].Value })
    $record.stages += @{ name = $Name; arguments = $Arguments; exit_code = $code; tests = $tests; ignored = $ignored; log = "$Name.log"; sha256 = (Get-FileHash -LiteralPath $log -Algorithm SHA256).Hash.ToLowerInvariant() }
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
    Stage 'protocol-default' ($common + @('-p', 'vcp-protocol', '--', '--test-threads=1'))
    Stage 'protocol-exact' ($common + @('-p', 'vcp-protocol', '--features', 'serde_json/arbitrary_precision', '--', '--test-threads=1'))
    Stage 'store-default-codec' ($common + @('-p', 'vcp-store', '--test', 'persisted_json', '--', '--test-threads=1'))
    Stage 'store-exact-regression' ($common + @('-p', 'vcp-store', '--features', 'qualification,serde_json/arbitrary_precision', '--', '--test-threads=1'))
    Stage 'lifecycle' ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--lib', '--', '--test-threads=1'))
    Stage 'cli' ($common + @('-p', 'vcp-cli', '--features', 'qualification', '--lib', '--', '--test-threads=1'))
    foreach ($selection in @('fresh_process_history', 'history_retention::', 'backup_checkpoint::', 'mcp_content_coding::')) {
        $name = 'host-' + $selection.TrimEnd(':')
        Stage $name ($common + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'canonical_host', $selection, '--', '--test-threads=1'))
    }
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
