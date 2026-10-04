#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$root = Join-Path ([IO.Path]::GetTempPath()) ('vcp-deadline-reconcile-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$checks = 0
function Check($Condition, $Message) { if (-not $Condition) { throw $Message }; $script:checks++ }
function New-Bundle([bool]$Settled) {
    $scope = @{ workspace = 'workspace'; session = 'session'; task = 'task' }
    $ledger = @{ scope = $scope; currency = 'USD'; cap = '5000000'; active = '0'; unresolved = $(if ($Settled) { '0' } else { '2500000' })
        settled = $(if ($Settled) { '1500000' } else { '1000000' }); overrun = $false }
    $costs = @(@{ collection = 'ledger'; id = 'task'; visibility = 'available'; record = $ledger })
    if (-not $Settled) {
        $costs += @{ collection = 'reservation'; id = 'reservation'; visibility = 'available'; record = @{ scope = $scope; phase = 'reconciliation_pending'; attempt = 'attempt'; liability = '2500000' } }
        $costs += @{ collection = 'attempt'; id = 'attempt'; visibility = 'available'; record = @{ scope = $scope; phase = 'reconciliation_pending' } }
    }
    return @{ kind = 'inspection_bundle'; schema_version = 1; source_watermark = 'watermark'; task = @{ scope = $scope; state = 'paused' }
        agents = @(@{ root = 'task'; total = 0; items = @(); next_offset = $null })
        views = @{
            costs = @(@{ scope = $scope; view = 'costs'; source_watermark = 'watermark'; gaps = @(); next_cursor = $null; items = $costs })
            tools = @(@{ scope = $scope; view = 'tools'; source_watermark = 'watermark'; gaps = @(); next_cursor = $null
                items = @(@{ collection = 'effect'; visibility = 'available'; record = @{ scope = $scope; state = 'succeeded' } }) })
        }
    }
}
function New-Fixture([string]$Scenario = 'A') {
    $fixtureRoot = Join-Path $root ([guid]::NewGuid().ToString('N'))
    $stage = if ($Scenario -eq 'A') { 'T5-production' } else { 'T5-concurrency' }
    [void][IO.Directory]::CreateDirectory((Join-Path $fixtureRoot $stage))
    $stdout = Join-Path $fixtureRoot "$stage/run.jsonl"; $stderr = Join-Path $fixtureRoot "$stage/run.stderr"
    $scope = [pscustomobject]@{ workspace = 'workspace'; session = 'session'; task = 'task' }
    $accepted = [pscustomobject]@{ type = 'accepted'; schema_version = 1; scope = $scope; correlation = 'original' }
    $result = [pscustomobject]@{ type = 'result'; schema_version = 1; scope = $scope; correlation = 'original'; exit_code = 7; conditions = @{ unresolved_effect = $true } }
    Write-Utf8File $stdout (($accepted, $result | ForEach-Object { $_ | ConvertTo-Json -Depth 10 -Compress }) -join "`n")
    Write-Utf8File $stderr ''
    Write-JsonFile (Join-Path $fixtureRoot "$stage/inspection-bundle.json") (New-Bundle $false)
    $run = [pscustomobject]@{ ExitCode = 7; TimedOut = $false; InvalidLines = 0; Accepted = $accepted; Result = $result; Frames = @($accepted, $result); Scope = $scope; StdoutPath = $stdout; StderrPath = $stderr }
    $record = [pscustomobject]@{ stage = $stage; task = 'task'; session = 'session'; budget_usd = [decimal]5; exit_code = 7; accepted_exit = @(0, 3, 8)
        conditions = @('unresolved_effect'); Run = $run; cost_usd = $null; kind = 'run'; autonomy = 'autonomous'; skipped = $null }
    $ctx = @{ Name = $(if ($Scenario -eq 'A') { 'a-vue-taskboard' } else { 'b-aspnet-inventory' }); Root = $fixtureRoot; Logs = $fixtureRoot; Results = $fixtureRoot; Workspace = $fixtureRoot
        ProgressLog = Join-Path $fixtureRoot 'progress.log'; Gates = [Collections.Generic.List[object]]::new(); Stages = [Collections.Generic.List[object]]::new(); Notes = [Collections.Generic.List[string]]::new()
        SpentUsd = [decimal]5; MaxScenarioUsd = [decimal]30; CostUnknown = $true; UnscopedCostUnknown = $false; UnknownTaskCosts = @{ task = $true }
        AccountedTaskUsd = @{ task = [decimal]5 }; SettledTaskUsd = @{ task = [decimal]0 }; TaskBudgetUsd = @{ task = [decimal]5 }
        PaidExecutionBlock = [pscustomobject]@{ task = 'task'; session = 'session'; stage = $stage; reasons = @('unresolved_effect'); approval_ids = @(); resume_same_task = $false; task_reason = ''; inspection = $fixtureRoot }
    }
    $ctx.Stages.Add($record)
    Test-StageExit $ctx $record $stage -DiagnosticUnresolvedDeadline
    return @{ Context = $ctx; Record = $record; OriginalHash = Get-Sha256 $stdout }
}
try {
    & $module {
        function script:Start-Sleep { param($Seconds); if ($Seconds -gt 30) { throw 'Unbounded metadata wait' }; $script:waits++ }
        function script:Invoke-Vcp {
            param($Ctx, $Stage, $Label, [string[]]$Arguments, $TimeoutSeconds, [switch]$DenyProviderCredentials)
            if ($TimeoutSeconds -ne 1800) { throw 'Unexpected metadata timeout' }
            if ($Arguments[0] -eq 'inspect-bundle') {
                if (-not $DenyProviderCredentials) { throw 'Fresh inspection received provider credentials' }
                $bundle = ($script:settledBundle | ConvertTo-Json -Depth 30 | ConvertFrom-Json -Depth 30)
                if ($script:mode -eq 'fresh-unresolved') { $bundle.views.costs[0].items[0].record.unresolved = '1' }
                if ($script:mode -eq 'tool-unknown') { $bundle.views.tools[0].items[0].record.state = 'unknown' }
                if ($script:mode -eq 'missing-page') { $bundle.views.tools[0].next_cursor = 'more' }
                if ($script:mode -eq 'fresh-scope') { $bundle.task.scope.task = 'another-task' }
                if ($script:mode -eq 'fresh-ledger-scope') { $bundle.views.costs[0].items[0].record.scope.session = 'another-session' }
                $exit = 0; $data = $bundle
            }
            elseif (($Arguments -join ' ') -eq 'tasks reconcile-cost task') {
                if ($DenyProviderCredentials) { throw 'Receipt metadata lost its required credential boundary' }
                $script:polls++
                $known = $script:mode -ne 'unknown' -and ($script:mode -ne 'retry' -or $script:polls -gt 1)
                $bundle = $(if ($known) { $script:settledBundle } else { $script:pendingBundle }) | ConvertTo-Json -Depth 30 | ConvertFrom-Json -Depth 30
                $data = [pscustomobject]@{ kind = 'provider_cost_reconciliation'; task = 'task'; scope = $bundle.task.scope; state = 'paused'; ledger = $bundle.views.costs[0].items[0].record
                    observations = @(@{ status = $(if ($known) { 'settled' } else { 'unknown' }); attempt = 'attempt' }); metadata_only = $true; resumed = $false }
                $exit = if ($known) { 0 } else { 7 }
                if ($script:mode -eq 'unsupported') { $exit = 2; $data = $null }
                if ($script:mode -eq 'scope') { $data.scope.task = 'another-task' }
                if ($script:mode -eq 'active') { $data.ledger.active = '1' }
                if ($script:mode -eq 'inference') { $data.metadata_only = $false }
                if ($script:mode -eq 'flag-string') { $data.metadata_only = 'true' }
            }
            else { throw "No paid command is allowed in this fixture: $($Arguments -join ' ')" }
            $scope = [pscustomobject]@{ workspace = 'workspace'; session = 'session'; task = 'task' }
            $result = [pscustomobject]@{ type = 'result'; schema_version = 1; scope = $scope; correlation = 'metadata'; exit_code = $exit; data = $data }
            $stdout = Join-Path $Ctx.Logs "$Stage/$Label.jsonl"
            Write-JsonFile $stdout $result
            return [pscustomobject]@{ ExitCode = $exit; TimedOut = $false; InvalidLines = $(if ($script:mode -eq 'malformed') { 1 } else { 0 }); Accepted = $null; Result = $result; Frames = @($result); StdoutPath = $stdout }
        }
    }
    foreach ($scenario in 'A', 'B') {
        foreach ($mode in 'settled', 'retry') {
            $fixture = New-Fixture $scenario; $ctx = $fixture.Context; $record = $fixture.Record
            & $module { param($m, $p, $s) $script:mode = $m; $script:pendingBundle = $p; $script:settledBundle = $s; $script:polls = 0; $script:waits = 0 } $mode (New-Bundle $false) (New-Bundle $true)
            $proof = Invoke-DeadlineCostReconciliation $ctx $record
            Check ($proof.original_exit_code -eq 7 -and $record.exit_code -eq 7 -and $record.accepted_exit -notcontains 7) 'Original exit/acceptance was rewritten'
            Check ((Get-Sha256 $record.Run.StdoutPath) -eq $fixture.OriginalHash -and $null -eq $record.cost_usd) 'Original native or accounting record was rewritten'
            Check ($ctx.SpentUsd -eq [decimal]1.5 -and -not $ctx.CostUnknown) 'Receipt settlement lost/double-counted cost'
            Check (@($ctx.Gates | Where-Object { $_.id -eq 'original-unresolved-exit' -and $_.outcome -eq 'fail' -and -not $_.required }).Count -eq 1) 'Original failure diagnostic missing'
            Check (@($ctx.Gates | Where-Object { $_.id -eq 'deadline-cost-reconciliation' -and $_.outcome -eq 'pass' -and $_.required }).Count -eq 1) 'Required reconciliation proof missing'
            Check ((& $module { param($c) Test-PaidExecutionAdmission $c ([ordered]@{}) @('resume', 'task') } $ctx)) 'Proven same-task resume denied'
            $blocked = $false
            try { & $module { param($c) Test-PaidExecutionAdmission $c ([ordered]@{ stage = 'other'; skipped = $null }) @('run') } $ctx } catch { $blocked = $true }
            Check $blocked 'Reconciliation permitted a fresh paid task'
            & $module { param($c) Update-ScenarioCost $c 'task' ([decimal]2) ([ordered]@{ stage = 'resume'; budget_usd = 5 }) } $ctx
            Check ($ctx.SpentUsd -eq 2) 'Resumed cumulative cost was counted twice'
            Check ((& $module { $script:polls }) -eq $(if ($mode -eq 'retry') { 2 } else { 1 })) 'Unexpected receipt polling count'
        }
    }
    foreach ($mode in 'unknown', 'unsupported', 'scope', 'active', 'inference', 'flag-string', 'malformed', 'fresh-unresolved', 'tool-unknown', 'missing-page', 'fresh-scope', 'fresh-ledger-scope') {
        $fixture = New-Fixture; $ctx = $fixture.Context; $record = $fixture.Record
        & $module { param($m, $p, $s) $script:mode = $m; $script:pendingBundle = $p; $script:settledBundle = $s; $script:polls = 0; $script:waits = 0 } $mode (New-Bundle $false) (New-Bundle $true)
        $failed = $false
        try { Invoke-DeadlineCostReconciliation $ctx $record | Out-Null } catch { $failed = $true }
        Check $failed "$mode incorrectly accepted reconciliation"
        Check ($ctx.SpentUsd -eq 5 -and $ctx.CostUnknown -and -not $ctx.PaidExecutionBlock.resume_same_task) "$mode released the full hold or permitted resume"
        Check (@($ctx.Gates | Where-Object { $_.id -eq 'deadline-cost-reconciliation' -and $_.required -and $_.outcome -eq 'fail' }).Count -eq 1) "$mode did not record required proof failure"
        if ($mode -eq 'unknown') { Check ((& $module { $script:polls }) -eq 3 -and (& $module { $script:waits }) -eq 2) 'Unknown receipts exceeded the bounded polling policy' }
    }
    foreach ($mode in 'original-framing', 'original-tool-unknown', 'unscoped-cost') {
        $fixture = New-Fixture; $ctx = $fixture.Context; $record = $fixture.Record
        & $module { $script:polls = 0 }
        if ($mode -eq 'original-framing') { ($ctx.Gates | Where-Object id -eq jsonl).outcome = 'fail' }
        elseif ($mode -eq 'unscoped-cost') { $ctx.UnscopedCostUnknown = $true }
        else {
            $bundle = New-Bundle $false
            $bundle.views.tools[0].items[0].record.state = 'unknown'
            Write-JsonFile (Join-Path $ctx.Logs "$($record.stage)/inspection-bundle.json") $bundle
        }
        $failed = $false
        try { Invoke-DeadlineCostReconciliation $ctx $record | Out-Null } catch { $failed = $true }
        Check ($failed -and (& $module { $script:polls }) -eq 0) "$mode reached metadata polling despite missing initial proof"
        Check ($ctx.SpentUsd -eq 5 -and $ctx.CostUnknown -and -not $ctx.PaidExecutionBlock.resume_same_task) "$mode changed the original hold or resume block"
    }
    # Exercise real continuation accounting and both finalizers. Only the native
    # process/inspection seam is inert; no provider call occurs in this test.
    $fixture = New-Fixture; $ctx = $fixture.Context; $record = $fixture.Record
    & $module { param($p, $s) $script:mode = 'settled'; $script:pendingBundle = $p; $script:settledBundle = $s; $script:polls = 0 } (New-Bundle $false) (New-Bundle $true)
    [void](Invoke-DeadlineCostReconciliation $ctx $record)
    $ctx.Started = Get-Date; $ctx.RunId = 'fixture'; $ctx.Vcp = 'C:\fixture\vcp.exe'; $ctx.VcpVersion = 'vcp 0.2.21'
    $ctx.DeadlineSeconds = 5; $ctx.SupportsInspectionBundle = $true; $ctx.SkipPaidStages = $false
    $ctx.Assets = [Collections.Generic.List[object]]::new()
    $resumedRun = $record.Run | ConvertTo-Json -Depth 30 | ConvertFrom-Json -Depth 30
    $resumedRun.ExitCode = 0; $resumedRun.Result.exit_code = 0; $resumedRun.Result.conditions = [pscustomobject]@{ completed = $true }
    $resumedRun.Frames = @($resumedRun.Accepted, $resumedRun.Result)
    $resumedRun | Add-Member -NotePropertyName DurationSeconds -NotePropertyValue 1
    $resumedRun | Add-Member -NotePropertyName EventCounts -NotePropertyValue @{}
    $settledBundle = New-Bundle $true
    $settledBundle.views.costs[0].items[0].record.settled = '2000000'
    & $module {
        param($run, $bundle)
        $script:resumedRun = $run; $script:resumedBundle = $bundle
        function script:Get-WorkspaceManifest { return @{} }
        function script:Invoke-Vcp {
            param($Ctx, $Stage, $Label, $Config, [string[]]$Arguments, $TimeoutSeconds, [switch]$Live)
            if (($Arguments -join ' ') -ne 'resume task') { throw 'Unexpected process during continuation fixture' }
            [void][IO.Directory]::CreateDirectory((Join-Path $Ctx.Logs $Stage))
            return $script:resumedRun
        }
        function script:Get-VcpStageInspection { return $script:resumedBundle.views }
    } $resumedRun $settledBundle
    $resumed = Invoke-VcpContinuation $ctx 'T5-resume' 'verified same-task continuation' @('resume', 'task') ''
    Test-StageExit $ctx $resumed 'T5-resume'
    [void](Invoke-Gate $ctx 'T5-resume' 'resume-same-task' 'same task' { $resumed.task -eq $record.task })
    Check ($resumed.cost_usd -eq [decimal]0.5 -and $ctx.SpentUsd -eq 2 -and -not $ctx.PaidExecutionBlock) 'Real continuation double-counted settlement or retained the resolved stop'
    Check ((Complete-VcpScenario $ctx) -eq 0) 'Verified continuation did not finalize successfully'
    $card = Get-Content -LiteralPath (Join-Path $ctx.Results 'scorecard.json') -Raw | ConvertFrom-Json -AsHashtable -Depth 100
    Check ($card.spend_evidence_complete -and $card.spend_usd -eq 2 -and $card.verdict -eq 'pass') 'Final scorecard lost cumulative settlement'
    Check ($card.stages[0].exit_code -eq 7 -and $null -eq $card.stages[0].cost_usd -and $card.stages[0].deadline_reconciliation.accounting.cost_usd -eq [decimal]1.5) 'Final scorecard rewrote original failure instead of retaining supplemental cost evidence'
    Import-Module (Join-Path $PSScriptRoot '../VcpAbCampaign.psm1') -Force
    $attempt = @{ scenario = 'A'; mode = 'Full'; status = 'running'; verdict = 'incomplete'; cap_usd = [decimal]30; liability_usd = [decimal]30; project = $ctx.Workspace; executable = $ctx.Vcp }
    Complete-CampaignAttempt $attempt $card 0 $false
    Check ($attempt.status -eq 'accounted' -and $attempt.verdict -eq 'pass' -and $attempt.liability_usd -eq 2) 'Campaign finalizer failed reconciled same-task accounting'
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    $prefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notlike 'vcp-deadline-reconcile-*') { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
Write-Host "Deadline reconciliation checks passed: $checks"
