#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '../VcpAbCampaign.psm1') -Force
$checks = 0
function Check([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message }; $script:checks++ }
function Must-Throw([scriptblock]$Action, [string]$Message) {
    $threw = $false
    try { & $Action } catch { $threw = $true }
    Check $threw $Message
}
function New-Attempt([decimal]$Cap = 30) {
    return @{ scenario = 'A'; mode = 'Full'; status = 'running'; verdict = 'incomplete'; cap_usd = $Cap; liability_usd = $Cap
        fingerprint = 'before'; output_fingerprint = 'after'; repair_note = ''; project = 'C:\project'; executable = 'C:\vcp.exe' }
}
function New-Card {
    return @{ schema = 'vcp-practical-scenario/1'; scenario = 'a-vue-taskboard'; dry_run = $false; max_scenario_usd = 30
        verdict = 'fail'; spend_usd = [decimal]0.48; spend_evidence_complete = $true; paid_execution_block = $null; gates = @()
        workspace = 'C:\project'; vcp = 'C:\vcp.exe' }
}
$state = @{ schema = 'vcp-ab-campaign/1'; authorized_usd = 100; attempts = @() }
$first = New-Attempt
Add-CampaignAttempt $state $first
Check ((Get-CampaignLiability $state) -eq 30) 'Launch must reserve its full cap.'
Must-Throw { Add-CampaignAttempt $state (New-Attempt) } 'Active launch allowed another launch.'
Complete-CampaignAttempt $first $null 1 $false
Check ($first.liability_usd -eq 30 -and $first.status -eq 'unresolved') 'Missing evidence released liability.'
Must-Throw { Add-CampaignAttempt $state (New-Attempt) } 'Unknown effects allowed another launch.'
$card = New-Card
$card.spend_evidence_complete = $false
Complete-CampaignAttempt $first $card 1 $false
Check ($first.liability_usd -eq 30) 'Incomplete accounting released liability.'
$card.spend_evidence_complete = $true
Complete-CampaignAttempt $first $card 1 $true
Check ($first.liability_usd -eq 30) 'Timeout released potentially active liability.'
Complete-CampaignAttempt $first $card 1 $false
Check ($first.liability_usd -eq [decimal]0.48 -and $first.status -eq 'accounted') 'Complete evidence did not reconcile spend.'
$retry = New-Attempt
Must-Throw { Add-CampaignAttempt $state $retry } 'Undiagnosed retry was admitted.'
$retry.repair_note = 'Fixed process environment.'
Must-Throw { Add-CampaignAttempt $state $retry } 'Unchanged inputs were admitted.'
$retry.fingerprint = 'after'
Must-Throw { Add-CampaignAttempt $state $retry } 'Unchanged terminal workspace was admitted.'
$retry.fingerprint = 'repaired'
Add-CampaignAttempt $state $retry
Check ((Get-CampaignLiability $state) -eq [decimal]30.48) 'Prior spend missing from next reservation.'
$retry.status = 'accounted'; $retry.verdict = 'pass'; $retry.liability_usd = 99
Add-CampaignAttempt $state (New-Attempt 1)
Check ($state.attempts.Count -eq 3) 'Legacy cumulative ceiling blocked approved refinement admission'
$invalid = New-Card; $invalid.spend_usd = 31
$attempt = New-Attempt
Must-Throw { Complete-CampaignAttempt $attempt $invalid 1 $false } 'Over-cap accounting was accepted.'
Check ($attempt.liability_usd -eq 30) 'Invalid evidence changed the reservation.'
$invalid = New-Card; $invalid.Remove('spend_usd')
Must-Throw { Complete-CampaignAttempt (New-Attempt) $invalid 1 $false } 'Missing spend was treated as zero.'
$invalid = New-Card; $invalid.workspace = 'C:\other'
Must-Throw { Complete-CampaignAttempt (New-Attempt) $invalid 1 $false } 'Unrelated workspace scorecard was accepted.'
# Current refinement observations do not revive historical financial ceilings.
foreach ($known in $false, $true) {
    $attempt = New-Attempt
    $attempt.effective_constraints = @{ spend = 'unbounded'; deadline = 'unbounded' }
    $card = New-Card
    $card.effective_constraints = @{ spend = 'unbounded'; deadline = 'unbounded' }
    $card.spend_usd = [decimal]125.25; $card.spend_evidence_complete = $known; $card.verdict = 'pass'
    $card.stages = @(@{ stage = 'T1'; task = 'task'; session = 'session'; exit_code = 0; skipped = $null })
    $card.gates = @(@{ stage = 'T1'; id = 'jsonl'; required = $true; outcome = 'pass' })
    Complete-CampaignAttempt $attempt $card 0 $false
    Check ($attempt.verdict -eq 'pass' -and $attempt.observed_spend_usd -eq [decimal]125.25) 'Verified quality or observed spend was changed by legacy caps'
    Check ($attempt.status -eq $(if ($known) { 'accounted' } else { 'observed' })) 'Billing completeness changed execution classification'
    $observations = Get-CampaignObservations @{ attempts = @($attempt) }
    Check ($observations.observed_spend_usd -eq [decimal]125.25 -and $observations.total_spend_known -eq $known) 'Observation total concealed financial uncertainty'
    foreach ($failure in 'timeout', 'missing-scope', 'missing-jsonl', 'inspection', 'effect', 'missing-card') {
        $bad = $card | ConvertTo-Json -Depth 20 | ConvertFrom-Json -AsHashtable
        switch ($failure) {
            'missing-scope' { $bad.stages[0].task = $null }
            'missing-jsonl' { $bad.gates = @() }
            'inspection' { $bad.gates += @{ stage = 'T1'; id = 'inspect-tools'; required = $true; outcome = 'fail'; detail = 'missing' } }
            'effect' { $bad.paid_execution_block = @{ reasons = @('unresolved_effect') } }
            'missing-card' { $bad = $null }
        }
        Complete-CampaignAttempt $attempt $bad 1 ($failure -eq 'timeout')
        Check ($attempt.status -eq 'unresolved' -and $attempt.verdict -eq 'incomplete' -and -not $attempt.spend_evidence_complete) "$failure was mistaken for financial-only uncertainty"
    }
}
# Extract only the test gate function, never execute the scenario's paid workflow.
$tokens = $null; $errors = $null
$tree = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../scenario-b-aspnet-inventory.ps1'), [ref]$tokens, [ref]$errors)
$function = $tree.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Test-Tests' }, $true)
. ([scriptblock]::Create($function.Extent.Text))
$script:invocations = 0
function Invoke-Dotnet { $script:invocations++; throw 'Old test assembly must never execute.' }
function Assert-That($condition, $message) { if (-not $condition) { throw $message } }
function Invoke-Gate { param($Ctx, $Stage, $Id, $Description, $Test); & $Test }
$ctx = @{ Gates = @(@{ stage = 'T1'; id = 'build'; outcome = 'fail' }) }
Must-Throw { Test-Tests 'T1' 1 } 'Failed build admitted --no-build tests.'
Check ($script:invocations -eq 0) 'Failed build executed stale tests.'
$ctx.Gates = @(@{ stage = 'old'; id = 'build'; outcome = 'pass' })
Must-Throw { Test-Tests 'T1' 1 } 'Previous-stage build admitted --no-build tests.'
Check ($script:invocations -eq 0) 'Previous-stage build executed stale tests.'
# Review must retain the configured admission headroom, including providers
# whose first request reservation alone exceeds the former fixed USD 2 cap.
$tree = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1'), [ref]$tokens, [ref]$errors)
$function = $tree.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Invoke-PlanModeReview' }, $true)
. ([scriptblock]::Create($function.Extent.Text))
function Get-WorkspaceManifest { return @{} }
function Invoke-VcpTask {
    param($Ctx, $Stage, $Title, $Prompt, $Config, $Autonomy, $BudgetUsd, $AcceptExit)
    $script:reviewBudget = $BudgetUsd
    $script:reviewAutonomy = $Autonomy
    return $null
}
[void](Invoke-PlanModeReview @{ Workspace = 'unused'; TurnBudgetUsd = [decimal]5 } 'T6-review' 'profile' 'Review')
Check ($script:reviewBudget -eq 5 -and $script:reviewAutonomy -eq 'plan') 'Read-only review changed configured budget headroom or autonomy.'
$function = $tree.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Invoke-RepairLoop' }, $true)
. ([scriptblock]::Create($function.Extent.Text))
function Get-FailedGates { return @(@{ id = 'build'; description = 'current build'; detail = 'compile error' }) }
$ctx = @{ MaxRepairTurns = 0; SkipPaidStages = $false }
Must-Throw { Invoke-RepairLoop $ctx 'T1' 'profile' { } } 'Exhausted repairs allowed dependent stages to advance.'
$ctx.SkipPaidStages = $true
Check ((Invoke-RepairLoop $ctx 'T1' 'profile' { }) -eq 'T1') 'Dry-run control flow was changed by paid repair stopping.'

$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-campaign-tests-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
try {
    $statePath = Join-Path $temporary 'campaign.json'
    $state = @{ schema = 'vcp-ab-campaign/1'; authorized_usd = 100; attempts = @() }
    Write-CampaignState $statePath $state
    $worker = Join-Path $temporary 'worker.ps1'
    [IO.File]::WriteAllText($worker, @'
param($Module, $Root, $Id)
$ErrorActionPreference = 'Stop'
Import-Module $Module -Force
$lock = Open-CampaignLock (Join-Path $Root 'campaign.lock')
try {
    $path = Join-Path $Root 'campaign.json'
    $state = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable
    $attempt = @{ id = $Id; scenario = 'A'; project = "project-$Id"; mode = 'Full'; status = 'accounted'; verdict = 'pass'
        cap_usd = 30; liability_usd = 30; fingerprint = $Id; output_fingerprint = $Id; repair_note = '' }
    try { Add-CampaignAttempt $state $attempt } catch { exit 13 }
    Start-Sleep -Milliseconds 100
    Write-CampaignState $path $state
}
finally { $lock.Dispose() }
# Finalize an independent field after releasing and reacquiring the lock, as
# the controller does after its child exits; every update must survive.
$lock = Open-CampaignLock (Join-Path $Root 'campaign.lock')
try {
    $state = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable
    ($state.attempts | Where-Object id -eq $Id).completed_marker = $true
    Write-CampaignState $path $state
}
finally { $lock.Dispose() }
'@)
    $children = @()
    foreach ($id in 1..6) {
        $start = [Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path)
        $start.UseShellExecute = $false; $start.CreateNoWindow = $true
        foreach ($argument in @('-NoProfile', '-NonInteractive', '-File', $worker, (Join-Path $PSScriptRoot '../VcpAbCampaign.psm1'), $temporary, "$id")) { $start.ArgumentList.Add($argument) }
        $children += [Diagnostics.Process]::Start($start)
    }
    foreach ($child in $children) { Check ($child.WaitForExit(15000)) 'Concurrent reservation worker hung.' }
    $codes = @($children | ForEach-Object ExitCode)
    Check (@($codes | Where-Object { $_ -eq 0 }).Count -eq 6 -and @($codes | Where-Object { $_ -eq 13 }).Count -eq 0) 'Concurrent admission was limited by the suspended financial ceiling.'
    $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
    Check ((Get-CampaignLiability $state) -eq 180 -and $state.attempts.Count -eq 6) 'Concurrent admission lost entries.'
    Check (@($state.attempts | Where-Object completed_marker -eq $true).Count -eq 6) 'Concurrent finalization lost another attempt update.'
    $active = @{ schema = 'vcp-ab-campaign/1'; authorized_usd = 100; attempts = @(@{ id = 'interrupted'; status = 'running'; project = $temporary }) }
    Must-Throw { Assert-CampaignActiveAttempts $active $temporary } 'An orphan reservation was treated as an active supervisor.'
    $projectLock = Open-CampaignLock (Get-CampaignProjectLock $temporary $temporary) 0
    try {
        Assert-CampaignActiveAttempts $active $temporary
        Must-Throw { Open-CampaignLock (Get-CampaignProjectLock $temporary $temporary) 0 } 'Concurrent access to an active workspace was admitted.'
    }
    finally { $projectLock.Dispose() }
}
finally {
    if ($children) { foreach ($child in $children) { if (-not $child.HasExited) { $child.Kill($true) }; $child.Dispose() } }
    $resolved = [IO.Path]::GetFullPath($temporary)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notlike 'vcp-campaign-tests-*') { throw 'Unsafe temporary cleanup path.' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
Write-Host "Campaign checks passed: $checks"
