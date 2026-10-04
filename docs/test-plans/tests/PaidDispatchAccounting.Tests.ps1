#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Exercise paid entry points with an inert process/evidence boundary.
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase ('vcp-paid-dispatch-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
function Check($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function New-Context {
    @{
        Name = 'dispatch'; RunId = 'fixture'; Started = Get-Date; Logs = $root; Results = $root; Workspace = $root
        ProgressLog = Join-Path $root 'progress.log'; DeadlineSeconds = 1; SupportsInspectionBundle = $true
        Gates = [Collections.Generic.List[object]]::new(); Stages = [Collections.Generic.List[object]]::new()
        Notes = [Collections.Generic.List[string]]::new(); Assets = [Collections.Generic.List[object]]::new()
        SpentUsd = [decimal]0; MaxScenarioUsd = [decimal]5; TurnBudgetUsd = [decimal]5
        AccountedTaskUsd = @{}; SettledTaskUsd = @{}; UnknownTaskCosts = @{}; TaskBudgetUsd = @{}
        CostUnknown = $false; UnscopedCostUnknown = $false; SkipPaidStages = $false
    }
}
try {
    & $module {
        function script:Get-FreshScenarioProfile { param($Ctx, $Stage, $Config) return $Config }
        function script:Invoke-Vcp {
            param($Ctx)
            if (-not $Ctx.CostUnknown) { throw 'Dispatch occurred before uncertainty was recorded.' }
            $script:dispatchObserved = $true
            if ($script:mode -eq 'parse') { throw 'Injected output decoding failure after process exit.' }
            return @{ Scope = @{ task = 'task'; session = 'session' }; Result = @{ conditions = @{} }; Frames = @(); ExitCode = 0; TimedOut = $false; DurationSeconds = 1; EventCounts = @{} }
        }
        # This is called by the real Complete-VcpStageEvidence before settlement.
        function script:Get-VcpStageInspection { throw 'Injected inspection failure after accepted paid execution.' }
    }
    foreach ($entry in 'run', 'resume', 'fork') {
        foreach ($failure in 'parse', 'inspect') {
            $ctx = New-Context
            if ($entry -ne 'run') {
                $record = [ordered]@{ stage = 'T1'; task = 'task'; session = 'session'; budget_usd = [decimal]5 }
                $ctx.Stages.Add([pscustomobject]$record)
                & $module { param($c, $r) Update-ScenarioCost $c 'task' ([decimal]1.25) $r } $ctx $record
                if ($entry -eq 'fork') { $ctx.MaxScenarioUsd = 10 }
            }
            & $module { param($m) $script:mode = $m; $script:dispatchObserved = $false } $failure
            $errorText = $null
            try {
                if ($entry -eq 'run') { [void](Invoke-VcpTask $ctx 'T2' 'fixture' 'fixture' 'profile') }
                elseif ($entry -eq 'resume') { [void](Invoke-VcpContinuation $ctx 'T2' 'fixture' @('resume', 'task') 'profile') }
                else {
                    $profile = Join-Path $root 'fork-profile.json'
                    Write-Utf8File $profile '{"budget_usd":"3.75"}'
                    [void](Invoke-VcpContinuation $ctx 'T2' 'fixture' @('sessions', 'fork', 'session') $profile)
                }
            }
            catch { $errorText = $_.Exception.Message }
            Check ($errorText -like 'Injected *') "$entry/$failure did not reach injected boundary: $errorText"
            Check ((& $module { $script:dispatchObserved })) 'Process boundary was not exercised'
            Check ($ctx.SpentUsd -eq $(if ($entry -eq 'run') { 0 } else { 1.25 }) -and $ctx.CostUnknown -and $ctx.UnscopedCostUnknown) "$entry/$failure lost or double-counted observed spend or uncertainty"
            Check ((Complete-VcpScenario $ctx) -eq 1) 'Accounting interruption passed finalization'
            $card = Get-Content -LiteralPath (Join-Path $root 'scorecard.json') -Raw | ConvertFrom-Json
            Check (-not $card.spend_evidence_complete -and $card.spend_usd -eq $ctx.SpentUsd) 'Final scorecard released interrupted spend'
        }
    }
    # Settlement followed by an artifact failure preserves the observation and unknown outcome.
    # Successful settlement counts cumulative
    # resumed-task usage once, through the existing accounting implementation.
    foreach ($failure in $false, $true) {
        $ctx = New-Context
        $record = [ordered]@{ stage = 'T1'; task = 'task'; session = 'session'; budget_usd = [decimal]5 }
        & $module { param($c, $r) Update-ScenarioCost $c 'task' ([decimal]1.25) $r } $ctx $record
        $caught = $false
        try {
            & $module {
                param($c, $r, $fail)
                Invoke-PaidScenarioDispatch $c ([decimal]3.75) {
                    Update-ScenarioCost $c 'task' ([decimal]1.5) $r
                    if ($fail) { throw 'Injected post-settlement artifact failure.' }
                }
            } $ctx $record $failure
        }
        catch { $caught = $true }
        Check ($caught -eq $failure) 'Unexpected settlement result'
        if ($failure) { Check ($ctx.SpentUsd -eq [decimal]1.5 -and $ctx.CostUnknown -and $ctx.UnscopedCostUnknown) 'Partial settlement was lost or unknown outcome erased' }
        else { Check ($ctx.SpentUsd -eq [decimal]1.5 -and -not $ctx.CostUnknown) 'Successful resume retained a duplicate hold' }
    }
    Write-Host 'Paid dispatch accounting passed: run/resume/fork parsing and inspection failures retain observations and explicit unknowns; unresolved execution cannot pass; cumulative settlement counted once.'
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    if (-not $resolved.StartsWith($tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        (Split-Path -Leaf $resolved) -notlike 'vcp-paid-dispatch-*') { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
