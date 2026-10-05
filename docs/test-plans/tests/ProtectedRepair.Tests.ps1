#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -DisableNameChecking
Import-Module (Join-Path $PSScriptRoot '../VcpCampaignRepair.psm1') -Force
$checks = 0
function Check($Value, $Message) { if (-not $Value) { throw $Message }; $script:checks++ }
function Reject([scriptblock]$Action, $Message) { $threw = $false; try { & $Action } catch { $threw = $true }; Check $threw $Message }
$root = Join-Path ([IO.Path]::GetTempPath()) ('protected-repair-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
try {
    $workspace = Join-Path $root 'app'; $source = Join-Path $root 'source'
    [void][IO.Directory]::CreateDirectory("$workspace/tests")
    [void][IO.Directory]::CreateDirectory("$source/results")
    $binding = @{ scenario = 'A'; workspace = $workspace; source_root = $source; source_stage = 'T3-labels' }
    foreach ($field in 'card','profile','prompt') {
        $path = Join-Path $source "$field.txt"; [IO.File]::WriteAllText($path, $field)
        $binding[$field] = $path; $binding["${field}_sha256"] = Get-Sha256 $path
    }
    $health = Join-Path $workspace 'tests/health.test.ts'
    [IO.File]::WriteAllText($health, 'original health assertion')
    $original = Get-Sha256 $health
    $proof = Get-CampaignRepairProtectedFiles $binding
    Check ($proof.files['tests/health.test.ts'] -eq $original -and $proof.origins['tests/health.test.ts'] -like 'current-before-only:*') 'Historical absence was not explicitly recorded.'
    Assert-CampaignRepairProtectedFiles $proof; $checks++
    [IO.File]::WriteAllText($health, 'weakened')
    Reject { Assert-CampaignRepairProtectedFiles $proof } 'During-repair protected mutation passed.'
    Remove-Item -LiteralPath $health
    Reject { Assert-CampaignRepairProtectedFiles $proof } 'Protected deletion passed.'
    [IO.File]::WriteAllText($health, 'original health assertion')
    $baselinePath = Join-Path $source 'results/protected-files.json'
    $baseline = @{ schema = 'vcp-protected-files/1'; workspace = $workspace; files = @{ 'tests/health.test.ts' = $original } }
    Write-JsonFile $baselinePath $baseline
    $proof = Get-CampaignRepairProtectedFiles $binding
    Check ($proof.origins['tests/health.test.ts'] -eq 'source-full-baseline') 'Retained Full baseline ignored.'
    [IO.File]::WriteAllText($health, 'tampered before repair')
    $before = Get-CampaignRepairProtectedFiles $binding
    Reject { Assert-CampaignRepairProtectedFiles $before } 'Preexisting tampering replaced the source Full baseline.'
    [IO.File]::WriteAllText($health, 'original health assertion')
    [IO.File]::AppendAllText($baselinePath, ' ')
    Reject { Assert-CampaignRepairProtectedFiles $proof } 'Baseline mutation after capture passed.'
    $baseline.workspace = 'C:\wrong'; Write-JsonFile $baselinePath $baseline
    Reject { Get-CampaignRepairProtectedFiles $binding } 'Wrong-workspace source baseline accepted.'
    $baseline.workspace = $workspace; $baseline.files['../outside'] = $original; Write-JsonFile $baselinePath $baseline
    Reject { Get-CampaignRepairProtectedFiles $binding } 'Caller-controlled protected path accepted.'
    Remove-Item -LiteralPath $baselinePath
    $checkpoint = Join-Path $source 'checkpoints/retained'
    [void][IO.Directory]::CreateDirectory("$checkpoint/files/tests")
    $copy = Join-Path $checkpoint 'files/tests/health.test.ts'; Copy-Item -LiteralPath $health -Destination $copy
    $manifestPath = Join-Path $checkpoint 'manifest.json'
    Write-JsonFile $manifestPath @{ schema = 'vcp-source-checkpoint/1'; workspace = $workspace; files = @{ 'tests/health.test.ts' = $original } }
    $proof = Get-CampaignRepairProtectedFiles $binding
    Check ($proof.origins['tests/health.test.ts'] -eq 'source-full-checkpoint') 'Existing source Full checkpoint ignored.'
    Assert-CampaignRepairProtectedFiles $proof; $checks++
    [IO.File]::WriteAllText($health, 'tampered before repair')
    Reject { Assert-CampaignRepairProtectedFiles (Get-CampaignRepairProtectedFiles $binding) } 'Checkpoint-backed preexisting tampering passed.'
    [IO.File]::WriteAllText($health, 'original health assertion')
    [IO.File]::AppendAllText($copy, ' changed')
    Reject { Get-CampaignRepairProtectedFiles $binding } 'Tampered checkpoint bytes accepted.'
    [IO.File]::WriteAllText($copy, 'original health assertion')
    $regression = Join-Path $workspace 'tests/regressions.test.ts'; [IO.File]::WriteAllText($regression, 'protected regression')
    $proof = Get-CampaignRepairProtectedFiles $binding
    Check ($proof.files.ContainsKey('tests/regressions.test.ts')) 'Existing optional regression test left unprotected.'
    [IO.File]::WriteAllText($regression, 'weakened regression')
    Reject { Assert-CampaignRepairProtectedFiles $proof } 'Optional regression mutation passed.'
    Remove-Item -LiteralPath $regression
    $binding.source_stage = 'T4-regressions'
    Reject { Get-CampaignRepairProtectedFiles $binding } 'T4 missing protected regressions accepted.'
    $binding.source_stage = 'T3-labels'
    # Required gates fail without throwing away their retained evidence.
    $ctx = @{ Gates = [Collections.Generic.List[object]]::new(); Name = 'protected-test'; ProgressLog = (Join-Path $root 'progress.log') }
    [IO.File]::WriteAllText($health, 'changed')
    [void](Invoke-Gate $ctx 'P1-repair' 'protected-files' 'source baseline' { Assert-CampaignRepairProtectedFiles $proof; $true })
    Check ($ctx.Gates[0].required -and $ctx.Gates[0].outcome -eq 'fail') 'Protected mismatch was advisory or lost.'
    $child = Get-Content (Join-Path $PSScriptRoot '../run-campaign-repair.ps1') -Raw
    Check ($child.IndexOf("'P1-repair' 'protected-files'") -lt $child.IndexOf('$stage = Invoke-VcpTask')) 'Protected baseline check runs after paid dispatch.'
    Check ($child.IndexOf("'R1-targeted' 'protected-files'") -lt $child.IndexOf('Save-Checkpoint')) 'Checkpoint precedes protected after-check.'
    Check ((Get-Sha256 $binding.card) -eq $binding.card_sha256) 'Original scorecard changed.'
    Write-Host "Protected repair tests passed ($checks checks)."
}
finally { if ([IO.Path]::GetFullPath($root).StartsWith([IO.Path]::GetTempPath(), [StringComparison]::OrdinalIgnoreCase)) { Remove-Item -LiteralPath $root -Recurse -Force } }
