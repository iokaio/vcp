#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$root = Join-Path ([IO.Path]::GetTempPath()) ('vcp-inspection-repair-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory((Join-Path $root 'T1'))
[IO.File]::WriteAllText((Join-Path $root 'T1/prompt.md'), 'Implement the real behavior. Preserve protected files and tests.')
$checks = 0
function Check($Condition, $Message) { if (-not $Condition) { throw $Message }; $script:checks++ }
function New-Context {
    $ctx = @{
        Name = 'inspection-repair'; RunId = 'fixture'; Started = Get-Date; Root = $root
        Logs = $root; Results = $root; Workspace = $root; ProgressLog = Join-Path $root 'progress.log'
        Gates = [Collections.Generic.List[object]]::new(); Stages = [Collections.Generic.List[object]]::new()
        Notes = [Collections.Generic.List[string]]::new(); Assets = [Collections.Generic.List[object]]::new()
        MaxRepairTurns = 2; SkipPaidStages = $false; PaidExecutionBlock = $null
        CostUnknown = $false; UnscopedCostUnknown = $false; SpentUsd = [decimal]5; MaxScenarioUsd = [decimal]30
        AccountedTaskUsd = @{ task = [decimal]5 }; TaskBudgetUsd = @{ task = [decimal]5 }
    }
    $ctx.Stages.Add([pscustomobject]@{ stage = 'T1'; task = 'task'; kind = 'run'; autonomy = 'autonomous'; skipped = $null
        exit_code = 0; workspace_diff = @{ Added = @(); Modified = @('app.cs'); Removed = @() } })
    return $ctx
}
function Fail-Gate($Ctx, $Id) {
    $Ctx.Gates.Add([pscustomobject]@{ stage = 'T1'; id = $Id; required = $true; outcome = 'fail'; description = $Id; detail = 'fixture failure' })
}
try {
    & $module {
        $script:repairCalls = 0
        function script:Invoke-VcpTask {
            param($Ctx, $Stage)
            $script:repairCalls++
            $record = [pscustomobject]@{ stage = $Stage; task = 'repair-task'; workspace_diff = @{ Added = @(); Modified = @(); Removed = @() } }
            $Ctx.Stages.Add($record)
            return $record
        }
        function script:Test-StageExit { }
    }
    foreach ($case in 'inspect-costs', 'inspect-verification', 'inspect-tools', 'inspect-routing', 'inspect-policy', 'inspect-outputs') {
        $ctx = New-Context
        Fail-Gate $ctx $case
        if ($case -eq 'inspect-costs') { $ctx.CostUnknown = $true }
        $beforeCalls = & $module { $script:repairCalls }
        $errorText = $null
        try { [void](Invoke-RepairLoop $ctx 'T1' 'profile' { }) } catch { $errorText = $_.Exception.Message }
        Check ($errorText -like '*canonical inspection*') "$case did not stop with evidence diagnosis: $errorText"
        Check ((& $module { $script:repairCalls }) -eq $beforeCalls) "$case launched a paid application repair"
        Check ($ctx.SpentUsd -eq 5) "$case changed the retained possible spend"
        $ctx.Fatal = $errorText
        Check ((Complete-VcpScenario $ctx) -eq 1) "$case finalization did not preserve failure"
        $card = Get-Content -LiteralPath (Join-Path $root 'scorecard.json') -Raw | ConvertFrom-Json
        Check ($card.spend_usd -eq 5 -and $card.verdict -eq 'fail') "$case changed liability or verdict"
        if ($ctx.CostUnknown) { Check (-not $card.spend_evidence_complete) "$case cleared accounting uncertainty" }
    }
    $ctx = New-Context
    $ctx.CostUnknown = $true
    Fail-Gate $ctx 'build'
    $beforeCalls = & $module { $script:repairCalls }
    $final = Invoke-RepairLoop $ctx 'T1' 'profile' { param($stage)
        $ctx.Gates.Add([pscustomobject]@{ stage = $stage; id = 'build'; required = $true; outcome = 'pass'; description = 'build'; detail = '' })
    }
    Check ($final -eq 'T1-repair1' -and ((& $module { $script:repairCalls }) -eq $beforeCalls + 1)) 'Financial-only uncertainty blocked the successful application repair'
    Check $ctx.CostUnknown 'Application repair silently settled unknown accounting'
    $ctx = New-Context
    Fail-Gate $ctx 'build'
    $beforeCalls = & $module { $script:repairCalls }
    $errorText = $null
    try { Invoke-RepairLoop $ctx 'T1' 'profile' { param($stage)
        $ctx.Gates.Add([pscustomobject]@{ stage = $stage; id = 'inspect-tools'; required = $true; outcome = 'fail'; description = 'inspection'; detail = 'timeout after repair' })
    } } catch { $errorText = $_.Exception.Message }
    Check ($errorText -like '*canonical inspection*' -and ((& $module { $script:repairCalls }) -eq $beforeCalls + 1)) 'Inspection failure after a genuine repair admitted another paid repair'
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    $prefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notlike 'vcp-inspection-repair-*') { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
Write-Host "Inspection repair checks passed: $checks"
