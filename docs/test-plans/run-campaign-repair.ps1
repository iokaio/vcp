#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Internal child of run-ab-campaign.ps1: one reserved repair, never a Full verdict.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$BindingPath,
    [Parameter(Mandatory)][string]$Vcp,
    [Parameter(Mandatory)][string]$ProjectPath,
    [Parameter(Mandatory)][string]$RunRoot,
    [Parameter(Mandatory)][string]$ProviderGeneration,
    [decimal]$TurnBudgetUsd = 5,
    [int]$OutputTokens = 8192,
    [int]$MaxRequests = 96,
    [int]$DeadlineSeconds = 1800,
    [switch]$AllowProcessPublish
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
Import-Module (Join-Path $PSScriptRoot 'VcpCampaignRepair.psm1') -Force
if (-not $AllowProcessPublish) { throw 'Explicit process authorization is required for repair.' }
$binding = Get-Content -LiteralPath $BindingPath -Raw | ConvertFrom-Json -AsHashtable -Depth 100
Assert-CampaignRepairInputs $binding
if ($binding.workspace -ine [IO.Path]::GetFullPath($ProjectPath)) { throw 'Repair binding workspace mismatch.' }
$ctx = Initialize-VcpScenario -Name ($binding.scenario.ToLowerInvariant() + '-targeted-repair') -RunRoot $RunRoot -ProjectPath $ProjectPath `
    -Vcp $Vcp -ProviderGeneration $ProviderGeneration -TurnBudgetUsd $TurnBudgetUsd -MaxScenarioUsd $TurnBudgetUsd `
    -MaxRepairTurns 0 -OutputTokens $OutputTokens -MaxRequests $MaxRequests -DeadlineSeconds $DeadlineSeconds -AllowProcessPublish
try {
    $protected = Get-CampaignRepairProtectedFiles $binding
    Write-JsonFile (Join-Path $ctx.Results 'protected-files.json') $protected
    [void](Invoke-Gate $ctx 'P1-repair' 'protected-files' 'Protected files match the source Full baseline where retained' {
        Assert-CampaignRepairProtectedFiles $protected
        $true
    })
    if (@(Get-FailedGates $ctx 'P1-repair').Count) { throw 'Protected source baseline changed; refusing paid repair.' }
    $source = Get-Content -LiteralPath $binding.profile -Raw | ConvertFrom-Json -AsHashtable -Depth 100
    Invoke-CommonPreflight -Ctx $ctx -AffectedPath $source.affected_paths[0]
    $profilePath = New-CampaignRepairProfile $ctx $binding
    Test-ProfileCheck -Ctx $ctx -Stage 'P1-repair' -Config $profilePath -Id 'original-contract'
    $preparedProfile = Get-Content -LiteralPath $profilePath -Raw | ConvertFrom-Json -AsHashtable -Depth 100
    foreach ($process in $preparedProfile.processes) {
        [void](Test-ProcessEnvironment -Ctx $ctx -Stage 'P1-repair' -Process $process -Id $process.name -Arguments @('--version'))
    }
    Assert-CampaignRepairInputs $binding
    $prompt = [IO.File]::ReadAllText($binding.prompt) + "`n`nPreserve the original named tests and acceptance checks. Do not weaken assertions or skip failing tests. Run the configured native vcp_verify checks after the final edit; review and repeat an executed:false response. Complete only with no outstanding issues. This is a targeted repair, not a full scenario qualification."
    $stage = Invoke-VcpTask -Ctx $ctx -Stage 'R1-targeted' -Title 'Single bounded targeted repair' -Prompt $prompt -Config $profilePath -BudgetUsd $TurnBudgetUsd
    if (-not $stage) { throw 'Reserved repair task did not execute.' }
    Test-StageExit $ctx $stage 'R1-targeted'
    [void](Invoke-Gate $ctx 'R1-targeted' 'protected-files' 'Protected source files remained byte-identical through repair' {
        Assert-CampaignRepairProtectedFiles $protected
        $true
    })
    [void](Invoke-Gate $ctx 'R1-targeted' 'native-verification' 'Current native checks passed for the original stage contract' {
        $bundle = Get-Content -LiteralPath (Join-Path $ctx.Logs 'R1-targeted/inspection-bundle.json') -Raw | ConvertFrom-Json -AsHashtable -Depth 100
        $profile = Get-Content -LiteralPath $profilePath -Raw | ConvertFrom-Json -AsHashtable -Depth 100
        Assert-CampaignRepairVerification $bundle $stage $profile
        $true
    })
    if ($ctx.UnscopedCostUnknown -or $ctx.PaidExecutionBlock -or @(Get-FailedGates $ctx 'R1-targeted').Count) { throw 'Repair failed verification or scoped execution evidence; no checkpoint or retry.' }
    Assert-CampaignRepairInputs $binding
    Save-Checkpoint $ctx ('Verified targeted repair of ' + $binding.source_attempt + '/' + $binding.source_stage)
}
catch { $ctx.Fatal = $_.Exception.Message; Write-Step $ctx $ctx.Fatal 'fail' }
finally { $exitCode = Complete-VcpScenario $ctx }
exit $exitCode
