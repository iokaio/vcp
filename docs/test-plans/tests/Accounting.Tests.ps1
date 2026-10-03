#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Offline accounting regressions; no CLI, provider, credential or network.
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-accounting-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
$checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
function New-Context {
    @{
        Name = 'accounting'; RunId = 'test'; Logs = $temporary; Results = $temporary
        ProgressLog = Join-Path $temporary 'progress.log'; Started = Get-Date
        Gates = [Collections.Generic.List[object]]::new(); Stages = [Collections.Generic.List[object]]::new()
        Notes = [Collections.Generic.List[string]]::new(); Assets = [Collections.Generic.List[object]]::new()
        SpentUsd = [decimal]0; MaxScenarioUsd = [decimal]3; TurnBudgetUsd = [decimal]3
        AccountedTaskUsd = @{}; SettledTaskUsd = @{}; UnknownTaskCosts = @{}; TaskBudgetUsd = @{}
        CostUnknown = $false; UnscopedCostUnknown = $false; SkipPaidStages = $false
    }
}
try {
    foreach ($value in [decimal]0.01, [decimal]3, [decimal]3.10) {
        & $module { param($v) Assert-ScenarioBudgetPrecision $v 'test' } $value
        Check ([decimal]::Parse((Format-Usd $value), [Globalization.CultureInfo]::InvariantCulture) -eq $value) 'Accepted budget changed during CLI formatting'
    }
    $rejected = $false
    try { & $module { Assert-ScenarioBudgetPrecision ([decimal]0.015) 'TurnBudgetUsd' } }
    catch { $rejected = $_.Exception.Message -like '*at most two decimal places*' }
    Check $rejected 'Sub-cent budget rounding could bypass admission accounting'
    $ctx = New-Context
    $record = [ordered]@{ stage = 'T5'; task = 'task-a'; session = 'session-a'; budget_usd = [decimal]3; cost_usd = $null }
    $ctx.Stages.Add([pscustomobject]$record)
    & $module { param($c, $r) Update-ScenarioCost $c 'task-a' ([decimal]1.25) $r } $ctx $record
    foreach ($arguments in @(@('resume', 'task-a'), @('resume', '--last'), @('sessions', 'resume', 'session-a'))) {
        $budget = & $module { param($c, $a) Get-ContinuationBudget $c $a '' } $ctx $arguments
        Check ($budget.Cap -eq 3 -and $budget.Additional -eq 1.75) 'Resume reserved a second full task cap'
        Check (($ctx.SpentUsd + $budget.Additional) -eq $ctx.MaxScenarioUsd) 'Resume would be skipped at its existing task ceiling'
    }
    & $module { param($c, $r) Update-ScenarioCost $c 'task-a' $null $r } $ctx $record
    Check ($ctx.SpentUsd -eq 3 -and $ctx.CostUnknown) 'Pause uncertainty did not reserve its task cap'
    $budget = & $module { param($c) Get-ContinuationBudget $c @('resume', 'task-a') '' } $ctx
    Check ($budget.Additional -eq 0) 'Uncertain paused task was double-reserved'
    & $module { param($c, $r) Update-ScenarioCost $c 'task-a' ([decimal]2) $r } $ctx $record
    Check ($ctx.SpentUsd -eq 2 -and $record.cost_usd -eq 0.75) 'Settlement did not reconcile the conservative reserve'
    Check (-not $ctx.CostUnknown -and $ctx.UnknownTaskCosts.Count -eq 0) 'Settled resume left task accounting unknown'
    [void](Add-GateResult $ctx T5 cost-evidence 'paused snapshot' fail 'unresolved at pause' $false)
    Check ((Complete-VcpScenario $ctx) -eq 0) 'Historical pause uncertainty failed a fully reconciled run'
    Check ((Get-Content (Join-Path $temporary 'scorecard.json') -Raw | ConvertFrom-Json).spend_evidence_complete) 'Final reconciled scorecard still marked costs incomplete'

    $ctx.UnscopedCostUnknown = $true
    & $module { param($c, $r) Update-ScenarioCost $c 'task-a' ([decimal]2) $r } $ctx $record
    Check ($ctx.CostUnknown) 'A later known task erased unscoped execution uncertainty'
    Check ((Complete-VcpScenario $ctx) -eq 1) 'Unscoped uncertainty passed final accounting'

    $ctx = New-Context
    $ctx.SpentUsd = [decimal]2.5
    $unscopedRecord = [ordered]@{ stage = 'resume'; budget_usd = [decimal]3; additional_budget_usd = [decimal]0.5; accepted_exit = @(0) }
    $run = [pscustomobject]@{ ExitCode = -1; DurationSeconds = 1; Scope = $null; Result = $null; Frames = @(); EventCounts = @{}; TimedOut = $true }
    [void](& $module { param($c, $r, $run) Complete-VcpStageEvidence $c 'resume' $run $null $r } $ctx $unscopedRecord $run)
    Check ($ctx.SpentUsd -eq 3) 'Unscoped resume double-counted its already reserved cap'
    Check ($ctx.UnscopedCostUnknown -and $ctx.CostUnknown) 'Unscoped execution did not retain sticky uncertainty'

    # main.rs emits exactly this unscoped result for rejected configuration;
    # a missing scope alone is insufficient evidence of zero dispatch.
    foreach ($case in 'rejected', 'timeout', 'invalid-json', 'accepted', 'extra-frame', 'wrong-exit', 'other-condition', 'missing-correlation') {
        $ctx = New-Context
        $ctx.SpentUsd = [decimal]0.331439
        $record = [ordered]@{ stage = 'repair'; budget_usd = [decimal]3; accepted_exit = @(0) }
        $frame = [pscustomobject]@{ type = 'result'; schema_version = 1; correlation = 'fixture-command'; scope = $null; receipt = $null; exit_code = 2; conditions = [pscustomobject]@{ invalid_configuration = $true } }
        $run = [pscustomobject]@{ ExitCode = 2; DurationSeconds = 0.2; Scope = $null; Result = $frame; Frames = @($frame); Accepted = $null; EventCounts = @{}; TimedOut = $false; InvalidLines = 0 }
        switch ($case) {
            'timeout' { $run.TimedOut = $true }
            'invalid-json' { $run.InvalidLines = 1 }
            'accepted' { $run.Accepted = @{ type = 'accepted' } }
            'extra-frame' { $run.Frames += @{ type = 'event' } }
            'wrong-exit' { $run.ExitCode = 1 }
            'other-condition' { $frame.conditions | Add-Member unresolved_effect $true }
            'missing-correlation' { $frame.correlation = '' }
        }
        [void](& $module { param($c, $r, $run) Complete-VcpStageEvidence $c 'repair' $run $null $r } $ctx $record $run)
        if ($case -eq 'rejected') {
            Check ($ctx.SpentUsd -eq [decimal]0.331439 -and -not $ctx.CostUnknown -and $record.cost_usd -eq 0) 'Proven pre-admission rejection invented a paid reservation'
            Check ($ctx.PaidExecutionBlock.reasons -contains 'invalid_configuration') 'Accounting classification cleared the execution failure'
        }
        else { Check ($ctx.SpentUsd -eq [decimal]3.331439 -and $ctx.CostUnknown) "$case lost conservative accounting" }
    }

    $ctx = New-Context
    $record = [ordered]@{ stage = 'T6'; task = 'review'; session = 'review-session'; budget_usd = [decimal]2; cost_usd = $null }
    $ctx.Stages.Add([pscustomobject]$record)
    & $module { param($c, $r) Update-ScenarioCost $c 'review' ([decimal]0.5) $r } $ctx $record
    $profile = Join-Path $temporary 'review-profile.json'
    Write-Utf8File $profile '{"budget_usd":"3.00"}'
    $budget = & $module { param($c, $p) Get-ContinuationBudget $c @('sessions', 'fork', 'review-session', '--through-turn', 'turn-a') $p } $ctx $profile
    Check ($budget.Cap -eq 3 -and $budget.Additional -eq 3) 'Fork used source run override instead of the actual selected profile cap'
    Write-Utf8File $profile '{"budget_usd":"2.00"}'
    $budget = & $module { param($c, $p) Get-ContinuationBudget $c @('sessions', 'fork', 'review-session', '--through-turn', 'turn-a') $p } $ctx $profile
    Check ($budget.Cap -eq 2 -and $budget.Additional -eq 2) 'Fork did not reserve a full new profile cap'
    & $module { param($c, $r) Update-ScenarioCost $c 'review' ([decimal]2.1) $r } $ctx $record
    Check ((Complete-VcpScenario $ctx) -eq 1) 'Observed task overspend passed final accounting'
    $ctx.MaxScenarioUsd = [decimal]1
    Check ((Complete-VcpScenario $ctx) -eq 1) 'Observed scenario overspend passed final accounting'
    Write-Host "PASS: $checks offline accounting checks"
}
finally {
    $full = [IO.Path]::GetFullPath($temporary)
    $parent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $full.StartsWith($parent, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $full -Leaf) -notlike 'vcp-accounting-tests-*') { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $full -Recurse -Force
}
