# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot, [string]$TargetRoot, [ValidateRange(1,16)][int]$Jobs = 2)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/p6-markov-qualification' }
if (-not $TargetRoot) { $TargetRoot = Join-Path $repository 'artifacts/cargo-target-heldout-order' }
$directory = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$record = [ordered]@{
    schema = 'p6-markov-qualification-run/1'; started_at = [DateTime]::UtcNow.ToString('o')
    status = 'running'; stages = @(); result = 'comparison.json'; live_requests = 0
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
    manifest_sha256 = (Get-FileHash -LiteralPath (Join-Path $repository 'src/evals/markov/manifest.json') -Algorithm SHA256).Hash.ToLowerInvariant()
    os = [Runtime.InteropServices.RuntimeInformation]::OSDescription
    architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
}
function Save-Record { $record | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $directory 'manifest.json') -Encoding utf8 }
function Identity {
    $value = & node (Join-Path $PSScriptRoot 'markov-source-identity.cjs')
    if ($LASTEXITCODE -ne 0) { throw 'Source identity capture failed' }
    return ($value | ConvertFrom-Json)
}
function Stage([string]$Name, [string[]]$Arguments) {
    $log = Join-Path $directory "$Name.log"
    & cargo @Arguments *> $log
    $code = $LASTEXITCODE
    $record.stages += @{name=$Name; arguments=$Arguments; exit_code=$code; log="$Name.log"; sha256=(Get-FileHash -LiteralPath $log -Algorithm SHA256).Hash.ToLowerInvariant()}
    Save-Record
    if ($code -ne 0) { throw "$Name failed; see $log" }
}
Save-Record
try {
    $record.source_before = Identity
    $record.rustc = (& rustc --version)
    $common = @('--manifest-path', (Join-Path $repository 'src/crates/vcp-models/Cargo.toml'), '--locked', '--offline', '--target-dir', ([IO.Path]::GetFullPath($TargetRoot)), '-j', "$Jobs")
    Stage 'gates' (@('test') + $common + @('--example','markov_qualification'))
    Stage 'comparison' (@('run') + $common + @('--example','markov_qualification','--',(Join-Path $repository 'src/evals/markov/manifest.json'),(Join-Path $directory 'comparison.json')))
    $record.result_sha256 = (Get-FileHash -LiteralPath (Join-Path $directory 'comparison.json') -Algorithm SHA256).Hash.ToLowerInvariant()
    $record.status = 'passed'
} catch {
    $record.status = 'failed'; $record.error = $_.Exception.Message
} finally {
    try {
        $record.source_after = Identity
        $record.source_unchanged = $record.Contains('source_before') -and $record.source_before.content_sha256 -eq $record.source_after.content_sha256 -and $record.source_before.commit -eq $record.source_after.commit
        if (-not $record.source_unchanged) { $record.status = 'failed'; $record.source_error = 'Relevant source changed during evaluation' }
    } catch { $record.status = 'failed'; $record.source_error = $_.Exception.Message }
    $record.ended_at = [DateTime]::UtcNow.ToString('o')
    Save-Record
}
Write-Output $directory
if ($record.status -ne 'passed') { throw 'Markov qualification runner failed; no evidence accepted' }
