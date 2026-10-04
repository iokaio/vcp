#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -DisableNameChecking
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase ('vcp-analysis-' + [guid]::NewGuid().ToString('N'))
try {
    $stage = Join-Path $root 'repair'
    New-Item -ItemType Directory -Path $stage -Force | Out-Null
    $bundlePath = Join-Path $stage 'inspection-bundle.json'
    $bundle = @{
        schema_version = 1; kind = 'inspection_bundle'; source_watermark = '1'
        task = @{ scope = @{ workspace = 'w'; session = 's'; task = 't' }; state = 'failed' }
        history = @(@{ rows = @(); next_cursor = $null })
        views = @{ verification = @(@{ items = @() }); costs = @(@{ items = @() }) }
    }
    Write-JsonFile $bundlePath $bundle
    $original = Get-Sha256 $bundlePath
    $ctx = @{ Logs = $root; Notes = [Collections.Generic.List[string]]::new() }
    $module = Get-Module VcpScenarioHarness
    1..2 | ForEach-Object { & $module { param($ctx, $path) Write-VcpExecutionAnalysis $ctx 'repair' $path } $ctx $bundlePath }
    $indexes = @(Get-ChildItem -LiteralPath $stage -Filter 'execution-analysis-index-*.json')
    Assert-That ($indexes.Count -eq 2) 'Repeated collection overwrote an earlier index'
    foreach ($index in $indexes) {
        $value = Get-Content -LiteralPath $index.FullName -Raw | ConvertFrom-Json
        Assert-That ($value.status -eq 'collected_pending_review') 'Offline analysis did not complete'
        Assert-That ($value.source_sha256 -eq $original) 'Bundle provenance changed'
        Assert-That ((Get-Sha256 (Join-Path $stage $value.source)) -eq $original) 'Immutable source snapshot was not preserved'
        Assert-That ($value.analysis_sha256 -eq (Get-Sha256 (Join-Path $stage $value.analysis))) 'Report provenance mismatch'
        $analysis = Get-Content -LiteralPath (Join-Path $stage $value.analysis) -Raw | ConvertFrom-Json
        Assert-That ($analysis.runs[0].analysis.facts.task_state -eq 'failed') 'Analysis rewrote task outcome'
        Assert-That ($value.quality_assessment -eq 'requires_independent_scenario_gates') 'Analysis claimed independent quality'
    }
    Assert-That ((Get-Sha256 $bundlePath) -eq $original) 'Source evidence was modified'
    Write-Utf8File $bundlePath '{"invalid":"fixture"}'
    & $module { param($ctx, $path) Write-VcpExecutionAnalysis $ctx 'repair' $path } $ctx $bundlePath
    $indexes = @(Get-ChildItem -LiteralPath $stage -Filter 'execution-analysis-index-*.json' | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json })
    Assert-That (@($indexes | Where-Object status -eq 'unavailable').Count -eq 1) 'Failed collection was hidden'
    Assert-That ($ctx.Notes.Count -eq 1) 'Missing analysis was not reported'
    Write-Host 'Execution analysis integration passed: immutable reports, provenance, failed outcomes and visible collection failure.'
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    if ($resolved.StartsWith($tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
