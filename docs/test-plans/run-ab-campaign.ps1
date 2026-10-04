#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Reserve, run, and collect one A/B attempt under the shared USD 100 campaign cap.
.DESCRIPTION
The supervising agent diagnoses and repairs between attempts. This controller
never launches an unchanged failed paid attempt automatically. Status is read-only.
Missing or unsettled evidence retains the entire reservation across restarts.
#>
[CmdletBinding()]
param(
    [ValidateSet('Status', 'Run', 'Repair', 'Reconcile', 'Quarantine', 'BoundQuarantine')][string]$Action = 'Status',
    [string]$CampaignRoot = 'C:\vcp-scenarios\ab-campaign-20261004',
    [ValidateSet('A', 'B')][string]$Scenario,
    [ValidateSet('DryRun', 'Full', 'Repair')][string]$Mode = 'DryRun',
    [string]$Vcp,
    [string]$ProjectPath,
    [string]$ProviderGeneration,
    [ValidateRange(0.01, 100)][decimal]$MaxAttemptUsd = 25,
    [ValidateRange(0.01, 100)][decimal]$TurnBudgetUsd = 5,
    [ValidateRange(0, 100)][int]$MaxRepairTurns = 2,
    [ValidateRange(1, 2147483647)][int]$OutputTokens = 8192,
    [ValidateRange(1, 2147483647)][int]$MaxRequests = 96,
    [ValidateRange(1, 86100)][int]$DeadlineSeconds = 1800,
    [ValidateRange(1, 86100)][int]$ShortDeadlineSeconds = 150,
    [ValidateRange(1, 86400)][int]$AttemptTimeoutSeconds = 21600,
    [switch]$AllowProcessPublish,
    [string]$RepairNote,
    [string]$AttemptId,
    [string]$SourceAttemptId,
    [string]$SourceStage,
    [string]$RepairPromptPath,
    [switch]$RefreshAccounting
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VcpAbCampaign.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
Import-Module (Join-Path $PSScriptRoot 'VcpCampaignRecovery.psm1') -Force -DisableNameChecking
Import-Module (Join-Path $PSScriptRoot 'VcpCampaignRepair.psm1') -Force
$CampaignRoot = [IO.Path]::GetFullPath($CampaignRoot)
$statePath = Join-Path $CampaignRoot 'campaign.json'
if ($Action -eq 'Status') {
    if (-not (Test-Path -LiteralPath $statePath -PathType Leaf)) { Write-Host "Campaign has no attempts: $statePath"; exit 0 }
    $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
    $liability = Get-CampaignLiability $state
    [pscustomobject]@{ authorized_usd = $state.authorized_usd; liability_usd = $liability; remaining_usd = 100 - $liability; attempts = $state.attempts } | ConvertTo-Json -Depth 32
    exit 0
}
if ($Action -in 'Reconcile', 'Quarantine', 'BoundQuarantine') {
    if (-not $AttemptId) { throw "$Action requires AttemptId; it reads retained evidence and never launches inference." }
    $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
    $matches = @($state.attempts | Where-Object { $_.id -eq $AttemptId })
    if ($matches.Count -ne 1) { throw 'AttemptId must match exactly one retained attempt.' }
    $lockedProject = $matches[0].project
    $projectLock = Open-CampaignLock (Get-CampaignProjectLock $CampaignRoot $matches[0].project) 0
    $lock = $null
    try {
        $lock = Open-CampaignLock (Join-Path $CampaignRoot 'campaign.lock')
        $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
        $matches = @($state.attempts | Where-Object { $_.id -eq $AttemptId })
        if ($matches.Count -ne 1) { throw 'AttemptId must match exactly one retained attempt.' }
        $attempt = $matches[0]
        if ($attempt.project -ine $lockedProject) { throw 'Attempt project changed while acquiring its lock.' }
        if ($attempt.status -eq 'billing-quarantined' -and $Action -ne 'BoundQuarantine') { throw 'This attempt has a permanent billing hold; reconciliation and workspace reuse are disabled.' }
        $cards = @(Get-ChildItem -LiteralPath $attempt.root -Filter scorecard.json -File -Recurse)
        if ($cards.Count -ne 1) { throw 'Exactly one completed scenario scorecard is required to reconcile this attempt.' }
        $card = Get-Content -LiteralPath $cards[0].FullName -Raw | ConvertFrom-Json -AsHashtable
        if ($Action -eq 'BoundQuarantine') {
            if (-not $Vcp) { throw 'BoundQuarantine requires an explicit local executable via -Vcp.' }
            if ($state.schema -ne 'vcp-ab-campaign/1' -or [decimal]$state.authorized_usd -ne 100) { throw 'Unsupported campaign authorization.' }
            Assert-CampaignActiveAttempts $state $CampaignRoot
            $baseline = $attempt | ConvertTo-Json -Depth 64 -Compress
            $scenarioRoot = Split-Path (Split-Path $cards[0].FullName -Parent) -Parent
            Assert-NoCampaignNativeProcess $scenarioRoot @(Get-CimInstance Win32_Process -ErrorAction Stop)
            $lock.Dispose(); $lock = $null
            $evidence = Invoke-CampaignQuarantineBound $attempt $card $scenarioRoot $Vcp $RepairNote
            Assert-NoCampaignNativeProcess $scenarioRoot @(Get-CimInstance Win32_Process -ErrorAction Stop)
            $evidence.native_processes_referencing_data = 0
            $evidence.project_lock_held = $true
            $path = Join-Path $evidence.root 'bounded-liability.json'
            $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
            try {
                $bytes = [Text.Encoding]::UTF8.GetBytes(($evidence | ConvertTo-Json -Depth 64))
                $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true)
            }
            finally { $stream.Dispose() }
            $lock = Open-CampaignLock (Join-Path $CampaignRoot 'campaign.lock')
            $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
            Assert-CampaignActiveAttempts $state $CampaignRoot
            $current = @($state.attempts | Where-Object id -eq $AttemptId)
            if ($state.schema -ne 'vcp-ab-campaign/1' -or [decimal]$state.authorized_usd -ne 100 -or $current.Count -ne 1 -or
                ($current[0] | ConvertTo-Json -Depth 64 -Compress) -cne $baseline) { throw 'Campaign or quarantined attempt changed during inspection; retain its allocation.' }
            foreach ($source in $evidence.evidence_files) {
                if ((Get-Sha256 $source.path) -ne $source.sha256) { throw 'Bound evidence changed before publication; retain its allocation.' }
            }
            # Unknown bills remain unknown. Keep the original cap, quarantine,
            # status and workspace isolation; retain every launched root cap.
            $current[0].liability_usd = $evidence.bounded_liability_usd
            $current[0].bounded_liability = @{ evidence = $path; sha256 = Get-Sha256 $path
                prior_allocation_usd = $evidence.prior_allocation_usd; retained_root_caps_usd = $evidence.bounded_liability_usd }
            Write-CampaignState $statePath $state
            Write-Host "Bounded $AttemptId to original root caps USD $($evidence.bounded_liability_usd); bills remain unknown and workspace remains isolated."
            exit 0
        }
        if ($Action -eq 'Reconcile' -and $RefreshAccounting) {
            if (-not $Vcp) { throw 'RefreshAccounting requires an explicit local executable via -Vcp.' }
            $scenarioRoot = Split-Path (Split-Path $cards[0].FullName -Parent) -Parent
            Assert-NoCampaignNativeProcess $scenarioRoot @(Get-CimInstance Win32_Process -ErrorAction Stop)
            # Keep the workspace locked while inspecting; release the campaign
            # lock so other supervised attempts can finalize their own records.
            $lock.Dispose(); $lock = $null
            $recovery = Invoke-CampaignAccountingRecovery $attempt $card $scenarioRoot $Vcp
            Assert-NoCampaignNativeProcess $scenarioRoot @(Get-CimInstance Win32_Process -ErrorAction Stop)
            $path = Join-Path $recovery.root 'accounting-recovery.json'
            $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
            try {
                $bytes = [Text.Encoding]::UTF8.GetBytes(($recovery | ConvertTo-Json -Depth 32))
                $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true)
            }
            finally { $stream.Dispose() }
            $lock = Open-CampaignLock (Join-Path $CampaignRoot 'campaign.lock')
            $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
            $current = @($state.attempts | Where-Object id -eq $AttemptId)
            if ($current.Count -ne 1 -or $current[0].status -ne 'unresolved' -or $current[0].liability_usd -ne $attempt.liability_usd -or
                $current[0].cap_usd -ne $attempt.cap_usd -or $current[0].project -ine $attempt.project -or $current[0].executable -ine $attempt.executable) {
                throw 'Attempt changed during metadata recovery; retain its allocation and inspect the new evidence.'
            }
            # Financial accounting alone is recovered. Original failure evidence
            # and the original scenario verdict remain unchanged.
            $current[0].status = 'accounted'
            $current[0].liability_usd = $recovery.settled_usd
            $current[0].accounting_recovery = @{ evidence = $path; sha256 = Get-Sha256 $path; settled_usd = $recovery.settled_usd }
            Write-CampaignState $statePath $state
            Write-Host "Recovered $AttemptId accounting: USD $($recovery.settled_usd); original scenario remains $($current[0].verdict)."
            exit 0
        }
        if ($Action -eq 'Quarantine') {
            $scenarioRoot = Split-Path (Split-Path $cards[0].FullName -Parent) -Parent
            if ((Split-Path $scenarioRoot -Leaf) -ne $card.run_id -or [DateTimeOffset]$card.started -lt [DateTimeOffset]$attempt.started -or
                [DateTimeOffset]$card.finished -lt [DateTimeOffset]$card.started) { throw 'Scorecard identity or timestamps do not belong to this attempt.' }
            # Read-only process inventory. Never persist command lines or environment values.
            Assert-NoCampaignNativeProcess $scenarioRoot @(Get-CimInstance Win32_Process -ErrorAction Stop)
            $bundles = @(); $hashes = @(@{ path = $cards[0].FullName; sha256 = Get-Sha256 $cards[0].FullName })
            foreach ($group in @($card.stages | Where-Object { -not $_.skipped } | Group-Object task)) {
                $stage = $group.Group[-1]
                if ($stage.stage -notmatch '^[A-Za-z0-9_-]+$') { throw 'Unsafe stage evidence path.' }
                $path = Join-Path $scenarioRoot "logs/$($stage.stage)/inspection-bundle.json"
                $bundles += Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable -Depth 100
                $hashes += @{ path = $path; sha256 = Get-Sha256 $path }
            }
            $evidence = Assert-CampaignBillingQuarantine $attempt $card $bundles $RepairNote $scenarioRoot
            $evidence.evidence_files = $hashes
            $evidence.native_processes_referencing_data = 0
            $path = Join-Path $attempt.root ('billing-quarantine-' + [guid]::NewGuid().ToString('N') + '.json')
            $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
            try {
                $bytes = [Text.Encoding]::UTF8.GetBytes(($evidence | ConvertTo-Json -Depth 32))
                $stream.Write($bytes, 0, $bytes.Length)
                $stream.Flush($true)
            }
            finally { $stream.Dispose() }
            $attempt.status = 'billing-quarantined'
            $attempt.quarantine = @{ evidence = $path; sha256 = Get-Sha256 $path; permanent_hold_usd = $attempt.cap_usd }
            Write-CampaignState $statePath $state
            Write-Host "Quarantined $AttemptId with permanent USD $($attempt.cap_usd) allocation. Original workspace/task must remain isolated."
            exit 0
        }
        if ($card.spend_evidence_complete -ne $true) { throw 'Retained scorecard still has incomplete accounting; retain the full reservation and inspect task costs.' }
        $attempt.scorecard = $cards[0].FullName
        Complete-CampaignAttempt $attempt $card $(if ($card.verdict -in 'pass', 'dry-run-pass') { 0 } else { 1 }) $false
        Write-CampaignState $statePath $state
        Write-CampaignState (Join-Path $attempt.root 'failure-bundle.json') $attempt
        Write-Host "Reconciled $AttemptId; campaign liability USD $(Get-CampaignLiability $state)."
    }
    finally { if ($lock) { $lock.Dispose() }; $projectLock.Dispose() }
    exit 0
}
if (-not $Scenario -or -not $ProjectPath -or -not $Vcp) { throw 'Run requires explicit Scenario, ProjectPath, and local Vcp executable.' }
if ($Mode -eq 'Repair' -and $Action -ne 'Repair') { throw 'Repair mode requires Action Repair and bound source evidence.' }
if ($Action -eq 'Repair') {
    $Mode = 'Repair'
    if (-not $SourceAttemptId -or -not $SourceStage -or -not $RepairPromptPath -or -not $ProviderGeneration -or -not $RepairNote -or $MaxAttemptUsd -ne $TurnBudgetUsd) {
        throw 'Repair requires SourceAttemptId, SourceStage, RepairPromptPath, ProviderGeneration, RepairNote, and equal MaxAttemptUsd/TurnBudgetUsd for its single task.'
    }
}
if ($Mode -in 'Full', 'Repair' -and -not $AllowProcessPublish) { throw 'Paid execution requires explicit process authorization through -AllowProcessPublish.' }
$Vcp = (Resolve-Path -LiteralPath $Vcp).Path
$ProjectPath = [IO.Path]::GetFullPath($ProjectPath)
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
function Get-CampaignFingerprint {
    $parts = [Collections.Generic.List[string]]::new()
    $parts.Add("executable=$(Get-Sha256 $Vcp)")
    foreach ($file in Get-ChildItem -LiteralPath $PSScriptRoot -File | Where-Object { $_.Extension -in '.ps1', '.psm1' } | Sort-Object Name) {
        $parts.Add("harness:$($file.Name)=$(Get-Sha256 $file.FullName)")
    }
    if (Test-Path -LiteralPath $ProjectPath) {
        $manifest = Get-WorkspaceManifest $ProjectPath
        foreach ($key in $manifest.Keys | Sort-Object) { $parts.Add("project:${key}=$($manifest[$key])") }
    }
    if ($ProviderGeneration) {
        foreach ($relative in 'snapshot.json', 'qualified/snapshot.json', 'endpoints.json') {
            $file = Join-Path $ProviderGeneration $relative
            if (Test-Path -LiteralPath $file -PathType Leaf) { $parts.Add("provider:${relative}=$(Get-Sha256 $file)") }
        }
    }
    $parts.Add("settings=$Scenario|$TurnBudgetUsd|$MaxAttemptUsd|$MaxRepairTurns|$OutputTokens|$MaxRequests|$DeadlineSeconds|$ShortDeadlineSeconds|$AllowProcessPublish")
    if ($Action -eq 'Repair') {
        $parts.Add("repair=$SourceAttemptId|$SourceStage|$(Get-Sha256 $RepairPromptPath)")
        if ($repairBinding) { $parts.Add("source=$($repairBinding.card_sha256)|$($repairBinding.profile_sha256)") }
    }
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes(($parts -join "`n")))).ToLowerInvariant()
}
[void][IO.Directory]::CreateDirectory($CampaignRoot)
# Hold the project lock across execution, but the ledger lock only while
# reserving/finalizing so independent A and B attempts can run concurrently.
$projectLock = Open-CampaignLock (Get-CampaignProjectLock $CampaignRoot $ProjectPath) 0
$lock = $null
$attempt = $null
$reserved = $false
$repairBinding = $null
try {
    $id = [DateTimeOffset]::UtcNow.ToString('yyyyMMdd-HHmmss') + "-$Scenario-" + [guid]::NewGuid().ToString('N').Substring(0, 8)
    $root = Join-Path $CampaignRoot "attempts/$id"
    $revision = & git -c "safe.directory=$repository" -C $repository rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot record repository revision.' }
    $sourceDiff = & git -c "safe.directory=$repository" -C $repository diff HEAD --no-ext-diff
    if ($LASTEXITCODE -ne 0) { throw 'Cannot record repository changes.' }
    $attempt = @{
        id = $id; scenario = $Scenario; mode = $Mode; status = 'running'; verdict = 'incomplete'
        started = [DateTimeOffset]::UtcNow.ToString('o'); finished = $null; root = $root
        cap_usd = $MaxAttemptUsd; liability_usd = $(if ($Mode -in 'Full', 'Repair') { $MaxAttemptUsd } else { 0 })
        executable = $Vcp; executable_sha256 = Get-Sha256 $Vcp; project = $ProjectPath
        revision = [string]$revision; source_diff_sha256 = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes(($sourceDiff -join "`n"))))
        fingerprint = Get-CampaignFingerprint; output_fingerprint = $null; repair_note = $RepairNote
        scorecard = $null; exit_code = $null; timed_out = $false; paid_execution_block = $null; failed_gates = @()
    }
    $lock = Open-CampaignLock (Join-Path $CampaignRoot 'campaign.lock')
    try {
        $state = if (Test-Path -LiteralPath $statePath) { Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable }
            else { @{ schema = 'vcp-ab-campaign/1'; authorized_usd = 100; created = [DateTimeOffset]::UtcNow.ToString('o'); attempts = @() } }
        Assert-CampaignActiveAttempts $state $CampaignRoot
        if ($Action -eq 'Repair') {
            $repairBinding = Get-CampaignRepairSource $state $SourceAttemptId $SourceStage $Scenario $ProjectPath $RepairPromptPath
            Assert-NoCampaignNativeProcess $repairBinding.source_root @(Get-CimInstance Win32_Process -ErrorAction Stop)
            $attempt.repair_source = $repairBinding
            $attempt.fingerprint = Get-CampaignFingerprint
        }
        Add-CampaignAttempt $state $attempt
        Write-CampaignState $statePath $state
        $reserved = $true
    }
    finally { $lock.Dispose(); $lock = $null }
    [void][IO.Directory]::CreateDirectory($root)
    if ($Action -eq 'Repair') {
        Assert-CampaignRepairInputs $repairBinding
        $bindingPath = Join-Path $root 'repair-source.json'
        Write-CampaignState $bindingPath $repairBinding
        foreach ($field in 'card', 'profile', 'prompt') { [IO.File]::Copy($repairBinding[$field], (Join-Path $root "source-$field.txt"), $false) }
    }
    $arguments = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $PSScriptRoot 'run-cli-scenarios.ps1'),
        '-Scenario', $Scenario, '-Mode', $Mode, '-Vcp', $Vcp, '-ProjectPath', $ProjectPath, '-RunRoot', $root,
        '-TurnBudgetUsd', $TurnBudgetUsd.ToString([cultureinfo]::InvariantCulture), '-MaxScenarioUsd', $MaxAttemptUsd.ToString([cultureinfo]::InvariantCulture),
        '-MaxRepairTurns', "$MaxRepairTurns", '-OutputTokens', "$OutputTokens", '-MaxRequests', "$MaxRequests",
        '-DeadlineSeconds', "$DeadlineSeconds", '-ShortDeadlineSeconds', "$ShortDeadlineSeconds")
    if ($ProviderGeneration) { $arguments += @('-ProviderGeneration', $ProviderGeneration) }
    if ($AllowProcessPublish) { $arguments += '-AllowProcessPublish' }
    if ($Action -eq 'Repair') {
        $arguments = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $PSScriptRoot 'run-campaign-repair.ps1'),
            '-BindingPath', $bindingPath, '-Vcp', $Vcp, '-ProjectPath', $ProjectPath, '-RunRoot', $root,
            '-ProviderGeneration', $ProviderGeneration, '-TurnBudgetUsd', $TurnBudgetUsd.ToString([cultureinfo]::InvariantCulture),
            '-OutputTokens', "$OutputTokens", '-MaxRequests', "$MaxRequests", '-DeadlineSeconds', "$DeadlineSeconds", '-AllowProcessPublish')
    }
    Write-Host "Attempt $id reserved USD $($attempt.liability_usd); campaign remaining USD $(100 - (Get-CampaignLiability $state))."
    $run = Invoke-NativeLogged -FilePath (Get-Process -Id $PID).Path -ArgumentList $arguments -WorkingDirectory $repository `
        -StdoutPath (Join-Path $root 'launcher.stdout.log') -StderrPath (Join-Path $root 'launcher.stderr.log') `
        -TimeoutSeconds $AttemptTimeoutSeconds -OnLine { param($line) Write-Host $line }
    $cards = @(Get-ChildItem -LiteralPath $root -Filter scorecard.json -File -Recurse)
    $card = $null
    if ($cards.Count -eq 1) {
        $attempt.scorecard = $cards[0].FullName
        $card = Get-Content -LiteralPath $cards[0].FullName -Raw | ConvertFrom-Json -AsHashtable
    }
    Complete-CampaignAttempt $attempt $card $run.ExitCode $run.TimedOut
    if ($Action -eq 'Repair') { Assert-CampaignRepairInputs $repairBinding }
    $attempt.output_fingerprint = Get-CampaignFingerprint
    $lock = Open-CampaignLock (Join-Path $CampaignRoot 'campaign.lock')
    try {
        $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
        for ($i = 0; $i -lt $state.attempts.Count; $i++) { if ($state.attempts[$i].id -eq $id) { $state.attempts[$i] = $attempt } }
        Write-CampaignState $statePath $state
    }
    finally { $lock.Dispose(); $lock = $null }
    Write-CampaignState (Join-Path $root 'failure-bundle.json') $attempt
    Write-Host "Attempt $id`: $($attempt.verdict); accounting $($attempt.status); campaign liability USD $(Get-CampaignLiability $state)."
    if ($run.ExitCode -ne 0 -or $attempt.verdict -notin 'pass', 'dry-run-pass', 'repair-pass') { exit 1 }
}
catch {
    if ($reserved) {
        $attempt.status = 'unresolved'
        $attempt.controller_failure = $_.Exception.Message
        # Never release a cap on controller error, even after partial parsing.
        $attempt.liability_usd = if ($Mode -in 'Full', 'Repair') { $MaxAttemptUsd } else { 0 }
        [void][IO.Directory]::CreateDirectory($root)
        Write-CampaignState (Join-Path $root 'failure-bundle.json') $attempt
        $lock = Open-CampaignLock (Join-Path $CampaignRoot 'campaign.lock')
        try {
            $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
            for ($i = 0; $i -lt $state.attempts.Count; $i++) { if ($state.attempts[$i].id -eq $id) { $state.attempts[$i] = $attempt } }
            Write-CampaignState $statePath $state
        }
        finally { $lock.Dispose(); $lock = $null }
    }
    throw
}
finally { if ($lock) { $lock.Dispose() }; $projectLock.Dispose() }
