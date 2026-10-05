#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Exercise real paid admission/evidence/repair functions with an inert CLI boundary.
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporary = Join-Path $tempBase ('vcp-blocked-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
$checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }; $script:checks++
}
function New-Context {
    $root = Join-Path $temporary ([guid]::NewGuid().ToString('N'))
    $ctx = @{
        Name = 'blocked-tests'; RunId = 'fixture'; Started = Get-Date; Root = $root
        Workspace = Join-Path $root 'workspace'; Logs = Join-Path $root 'logs'; Results = Join-Path $root 'results'
        Gates = [Collections.Generic.List[object]]::new(); Stages = [Collections.Generic.List[object]]::new()
        Notes = [Collections.Generic.List[string]]::new(); Assets = [Collections.Generic.List[object]]::new()
        TurnBudgetUsd = [decimal]3; MaxScenarioUsd = [decimal]30; SpentUsd = [decimal]0; DeadlineSeconds = 5; MaxRepairTurns = 1
        AccountedTaskUsd = @{}; SettledTaskUsd = @{}; UnknownTaskCosts = @{}; TaskBudgetUsd = @{}
        CostUnknown = $false; UnscopedCostUnknown = $false; SkipPaidStages = $false
    }
    foreach ($path in $ctx.Workspace, $ctx.Logs, $ctx.Results) { New-Item -ItemType Directory -Path $path -Force | Out-Null }
    $ctx.ProgressLog = Join-Path $ctx.Logs 'progress.log'
    $ctx.CommandLog = Join-Path $ctx.Logs 'vcp-commands.log'
    return $ctx
}
function Set-FixtureRun([string[]]$Conditions, [int]$Exit, [string]$Task = 'task-a', [switch]$Timeout, [switch]$MissingResult, [switch]$Approval) {
    $conditionsObject = [ordered]@{}
    foreach ($name in $Conditions) { $conditionsObject[$name] = $true }
    $scope = [pscustomobject]@{ workspace = 'workspace-a'; session = "session-$Task"; task = $Task }
    $accepted = [pscustomobject]@{ type = 'accepted'; scope = $scope; correlation = 'fixture-command' }
    $result = if ($MissingResult) { $null } else { [pscustomobject]@{ type = 'result'; conditions = [pscustomobject]$conditionsObject; exit_code = $Exit; scope = $scope; correlation = 'fixture-command' } }
    $frames = @($accepted)
    if ($Approval) { $frames += [pscustomobject]@{ type = 'required_input'; approval = 'approval-fixture' } }
    if ($result) { $frames += $result }
    $run = [pscustomobject]@{
        ExitCode = $Exit; DurationSeconds = 0.01; Result = $result; Accepted = $accepted; Frames = $frames
        Scope = $scope; TimedOut = [bool]$Timeout; EventCounts = @{}; InvalidLines = 0
        StdoutPath = Join-Path $temporary 'run.stdout.jsonl'; StderrPath = Join-Path $temporary 'run.stderr.txt'
    }
    & $module { param($r) $script:fixtureRun = $r } $run
}
function Paid-Count { & $module { @($script:fixtureCalls | Where-Object { $_ -match '^(run |resume |sessions (resume|fork) )' }).Count } }
function Assert-Blocked([scriptblock]$Action, [string]$Description) {
    $count = Paid-Count
    $errorText = $null
    try { & $Action | Out-Null } catch { $errorText = $_.Exception.Message }
    Check ($errorText -like 'Scenario stopped before*') "$Description did not stop the scenario: $errorText"
    if ($ctx.PaidExecutionBlock.task_reason) {
        Check ($errorText.Contains($ctx.PaidExecutionBlock.task_reason)) "$Description hid the VCP task reason in its fatal message"
    }
    Check ((Paid-Count) -eq $count) "$Description executed another paid command"
}
try {
    & $module {
        $script:fixtureCalls = [Collections.Generic.List[string]]::new()
        # Admission/repair tests isolate profile renewal, covered by StagePreparation.Tests.
        function script:Get-FreshScenarioProfile { param($Ctx, $Stage, $Config) $Config }
        function script:Invoke-Vcp {
            param($Ctx, $Stage, $Label, $Config, [string[]]$Arguments, $TimeoutSeconds, [switch]$Live)
            $script:fixtureCalls.Add(($Arguments -join ' '))
            if ($Arguments[0] -in 'run', 'resume' -or ($Arguments[0] -eq 'sessions' -and $Arguments[1] -in 'resume', 'fork')) { return $script:fixtureRun }
            if ($Arguments[0] -eq 'inspect-bundle') {
                $scope = $script:fixtureRun.Scope
                $views = @{}
                foreach ($view in 'costs', 'verification', 'tools', 'routing', 'policy', 'outputs') {
                    $views[$view] = @([pscustomobject]@{ view = $view; scope = $scope; source_watermark = 'fixture-watermark'; items = @(); next_cursor = $null })
                }
                $bundle = [pscustomobject]@{
                    schema_version = 1; source_watermark = 'fixture-watermark'; views = $views
                    task = [pscustomobject]@{ scope = $scope; reason = "HTTP 429: upstream provider shared pool is rate limited`nAccounting remains unresolved." }
                    agents = @(); history = @()
                }
                return [pscustomobject]@{ ExitCode = 0; TimedOut = $false; InvalidLines = 0; Result = [pscustomobject]@{ data = $bundle } }
            }
            if (($Arguments[0..1] -join ' ') -eq 'tasks status') {
                return [pscustomobject]@{ ExitCode = 0; Result = [pscustomobject]@{ data = [pscustomobject]@{ reason = "HTTP 429: upstream provider shared pool is rate limited`nAccounting remains unresolved." } } }
            }
            return [pscustomobject]@{ ExitCode = 0; InvalidLines = 0; Result = [pscustomobject]@{ data = [pscustomobject]@{ items = @(); next_cursor = $null } } }
        }
        function script:Get-VcpTaskCost { param($Pages) [pscustomobject]@{ Usd = [decimal]0.2; Attempts = 1 } }
        function script:Get-VcpFinalMessage { param($Ctx, $Stage, $OutputPages, $Frames) '' }
    }
    foreach ($inspectionBundle in @($false, $true)) {
      foreach ($case in @(
        @{ conditions = @('required_input'); exit = 4; approval = $true },
        @{ conditions = @('unresolved_effect'); exit = 7 },
        @{ conditions = @('internal_failure'); exit = 1 },
        @{ conditions = @('cancelled'); exit = 6 },
        @{ conditions = @('invalid_configuration'); exit = 2 },
        @{ conditions = @('budget_exhausted'); exit = 5 },
        @{ conditions = @(); exit = 4; approval = $true },
        @{ conditions = @(); exit = -1; timeout = $true; missingResult = $true },
        @{ conditions = @(); exit = 0; missingResult = $true },
        @{ conditions = @('durably_paused', 'required_input'); exit = 4; approval = $true }
    )) {
        $ctx = New-Context
        $ctx.SupportsInspectionBundle = $inspectionBundle
        & $module { $script:fixtureCalls.Clear() }
        Set-FixtureRun @case
        $first = Invoke-VcpTask $ctx 'T1' 'Initial task' 'Use configured tools. Preserve tests.' 'fixture-profile' -AcceptExit @(0, 4)
        Check ($null -ne $first -and $null -ne $ctx.PaidExecutionBlock) 'Stopping command did not retain its stage and block'
        Check (Test-Path -LiteralPath (Join-Path $ctx.Logs 'T1/inspect-policy.json')) 'Policy evidence sweep did not finish'
        Check (Test-Path -LiteralPath (Join-Path $ctx.Logs 'T1/inspect-tools.json')) 'Tool evidence sweep did not finish'
        $inspectionCalls = @(& $module { $script:fixtureCalls.ToArray() })
        if ($inspectionBundle) {
            Check ($inspectionCalls -contains 'inspect-bundle task-a' -and $inspectionCalls -notcontains 'tasks status task-a') 'Bundled inspection fell back to legacy task status'
            Check (@($ctx.Gates | Where-Object { $_.id -like 'inspect-*' -and $_.outcome -ne 'pass' }).Count -eq 0) 'Bundled inspection fixture failed canonical completeness checks'
        }
        else { Check ($inspectionCalls -contains 'tasks status task-a' -and $inspectionCalls -notcontains 'inspect-bundle task-a') 'Legacy inspection did not exercise task-status result unwrapping' }
        $block = Get-Content -LiteralPath (Join-Path $ctx.Results 'paid-execution.json') -Raw | ConvertFrom-Json
        Check ($block.task_reason -like 'HTTP 429:*Accounting remains unresolved.' -and $block.task_reason -notmatch '[\x00-\x1f\x7f]') 'Durable task diagnostic was hidden or contained terminal controls'
        Check ($first.task_reason -eq $block.task_reason) 'Stage and stopping block disagree on the provider reason'
        Check ($block.task -eq 'task-a' -and -not $block.resume_same_task -and $block.commands -eq $ctx.CommandLog) 'Block lost task/command evidence or permits resume'
        if ($case.approval) { Check ($block.approval_ids -contains 'approval-fixture') 'Block lost approval ID' }
        [void](Add-GateResult $ctx 'T1' 'feature' 'Required feature' 'fail' 'fixture failure' $true)
        Assert-Blocked { Invoke-RepairLoop $ctx 'T1' 'fixture-profile' { throw 'repair assessment must not run' } } 'Repair after blocker'
        Assert-Blocked { Invoke-VcpTask $ctx 'T2' 'Later task' 'New work' 'fixture-profile' } 'New task after blocker'
        Assert-Blocked { Invoke-PlanModeReview $ctx 'T6' 'fixture-profile' 'Review' } 'Review after blocker'
        Assert-Blocked { Invoke-VcpContinuation $ctx 'T7' 'Fork' @('sessions', 'fork', 'session-task-a') 'missing-profile' } 'Fork after blocker'
        Assert-Blocked { Invoke-VcpContinuation $ctx 'T8' 'Resume' @('resume', 'task-a') 'fixture-profile' } 'Resume after non-deadline blocker'
        Check ((Complete-VcpScenario $ctx) -eq 1) 'Blocked scenario reported success'
        $scorecard = Get-Content -LiteralPath (Join-Path $ctx.Results 'scorecard.json') -Raw | ConvertFrom-Json
        Check ($scorecard.paid_execution_block.task -eq 'task-a') 'Final scorecard lost stopping task'
        Check ($scorecard.paid_execution_block.task_reason -eq $block.task_reason) 'Final scorecard lost the blocked provider reason'
        $stoppingGate = $scorecard.gates | Where-Object { $_.stage -eq 'FINAL-execution' -and $_.id -eq 'paid-execution-stopped' }
        Check ($stoppingGate.detail.Contains($block.task_reason)) 'Final stopping gate hid the VCP task reason'
      }
    }
    # A normal durable deadline permits only continuation of its own task.
    foreach ($arguments in @(@('resume', 'task-a'), @('resume', '--last'), @('sessions', 'resume', 'session-task-a'))) {
        $ctx = New-Context
        Set-FixtureRun @('durably_paused') 8
        [void](Invoke-VcpTask $ctx 'T5' 'Deadline task' 'Complete task' 'fixture-profile' -AcceptExit @(0, 8))
        Check ($ctx.PaidExecutionBlock.resume_same_task) 'Deadline did not permit scoped continuation'
        Assert-Blocked { Invoke-VcpTask $ctx 'T-new' 'New task' 'New work' 'fixture-profile' } 'New task during deadline pause'
        Assert-Blocked { Invoke-VcpContinuation $ctx 'T-wrong' 'Other task' @('resume', 'task-b') 'fixture-profile' } 'Wrong task during deadline pause'
        Assert-Blocked { Invoke-VcpContinuation $ctx 'T-fork' 'Fork' @('sessions', 'fork', 'session-task-a') 'missing-profile' } 'Fork during deadline pause'
        Set-FixtureRun @('completed') 0
        $resumed = Invoke-VcpContinuation $ctx 'T5-resume' 'Deadline continuation' $arguments 'fixture-profile'
        Check ($resumed.task -eq 'task-a' -and $null -eq $ctx.PaidExecutionBlock) 'Successful scoped resume did not clear deadline block'
        Check ($ctx.SpentUsd -eq [decimal]0.2) 'Same task continuation changed cumulative accounting semantics'
        Set-FixtureRun @('completed') 0 'task-b'
        Check ($null -ne (Invoke-VcpTask $ctx 'T-next' 'Next task' 'New work' 'fixture-profile')) 'New work did not resume after completed deadline task'
    }
    # A fresh repair receives original tools and protected-file constraints.
    $ctx = New-Context
    $original = "Use node with C:/tools/node_modules/npm/bin/npm-cli.js run typecheck.`nDo not edit tests/protected.test.ts or package-lock.json."
    Set-FixtureRun @('completed') 0
    [void](Invoke-VcpTask $ctx 'T1' 'Task with constraints' $original 'fixture-profile')
    [void](Add-GateResult $ctx 'T1' 'typecheck' 'Typecheck succeeds' 'fail' 'fixture compile error' $true)
    Set-FixtureRun @('completed') 0 'repair-task'
    $finished = Invoke-RepairLoop $ctx 'T1' 'fixture-profile' {
        param($stage)
        [void](Add-GateResult $ctx $stage 'typecheck' 'Typecheck succeeds' 'pass' '' $true)
    }
    $repairPrompt = [IO.File]::ReadAllText((Join-Path $ctx.Logs 'T1-repair1/prompt.md'))
    Check ($finished -eq 'T1-repair1') 'Ordinary completed-task repair no longer runs'
    Check ($repairPrompt.Contains($original)) 'Repair lost original environment/tool/protected-file instructions'
    Check ($repairPrompt.Contains('fixture compile error')) 'Repair lost independent failure evidence'
    Check ($repairPrompt.Contains('Authored files changed during the preceding attempt')) 'Repair lost observed implementation evidence'
    Check (($ctx.Stages | Where-Object stage -eq 'T1-repair1').accepted_exit -notcontains 3) 'Repair treats incomplete completion evidence as success'
    Write-Host "PASS: $checks offline blocked-execution and repair-context checks"
}
finally {
    Remove-Module $module -Force
    $resolved = [IO.Path]::GetFullPath($temporary)
    $prefix = $tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $resolved -Leaf) -notlike 'vcp-blocked-tests-*') { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
