# SPDX-License-Identifier: Apache-2.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VcpCampaignProcessProof.psm1') -DisableNameChecking

function Write-CampaignState([string]$Path, $State) {
    $temporary = "$Path.$([guid]::NewGuid().ToString('N')).tmp"
    try {
        [IO.File]::WriteAllText($temporary, ($State | ConvertTo-Json -Depth 64), [Text.UTF8Encoding]::new($false))
        [IO.File]::Move($temporary, $Path, $true)
    }
    finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary } }
}

function Get-CampaignLiability($State) {
    $total = [decimal]0
    foreach ($attempt in $State.attempts) { $total += [decimal]$attempt.liability_usd }
    return $total
}

function Open-CampaignLock([string]$Path, [int]$WaitMilliseconds = 5000) {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while ($true) {
        try { return [IO.File]::Open($Path, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
        catch [IO.IOException] {
            if ($clock.ElapsedMilliseconds -ge $WaitMilliseconds) { throw }
            [Threading.Thread]::Sleep(50)
        }
    }
}

function Get-CampaignProjectLock([string]$Root, [string]$Project) {
    $canonical = [IO.Path]::GetFullPath($Project).TrimEnd('\', '/').ToUpperInvariant()
    $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($canonical)))
    return Join-Path $Root "project-$hash.lock"
}

function Assert-CampaignActiveAttempts($State, [string]$Root) {
    foreach ($active in @($State.attempts | Where-Object { $_.status -eq 'running' })) {
        $probe = $null
        try { $probe = Open-CampaignLock (Get-CampaignProjectLock $Root $active.project) 0 }
        catch [IO.IOException] { continue }
        if ($probe) { $probe.Dispose(); throw "Attempt $($active.id) lost its supervisor; reconcile its retained evidence before launching again." }
    }
}

function Add-CampaignAttempt($State, [hashtable]$Attempt) {
    if ($State.schema -ne 'vcp-ab-campaign/1') { throw 'Unsupported campaign format.' }
    if ([decimal]$Attempt.liability_usd -lt 0) { throw 'Negative observed campaign amount.' }
    # Never release a reservation merely because a process vanished or a run failed.
    if (@($State.attempts | Where-Object { $_.status -eq 'unresolved' }).Count) { throw 'An unresolved attempt requires evidence reconciliation before another launch.' }
    if (@($State.attempts | Where-Object { $_.status -eq 'running' -and $_.project -ieq $Attempt.project }).Count) { throw 'This project already has an active attempt.' }
    $project = [IO.Path]::GetFullPath($Attempt.project).TrimEnd('\', '/')
    foreach ($held in @($State.attempts | Where-Object { $_.status -eq 'billing-quarantined' })) {
        $old = [IO.Path]::GetFullPath($held.project).TrimEnd('\', '/')
        if ($project -ieq $old -or $project.StartsWith($old + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
            $old.StartsWith($project + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'A billing-quarantined workspace must remain isolated; select a fresh, independent project directory.'
        }
    }
    if ($Attempt.mode -in 'Full', 'Repair') {
        $previous = @($State.attempts | Where-Object { $_.scenario -eq $Attempt.scenario -and $_.mode -eq $Attempt.mode })
        if ($previous.Count -and $previous[-1].verdict -ne 'pass') {
            if (-not $Attempt.repair_note) { throw 'A failed paid retry requires a RepairNote describing the diagnosis and repair.' }
            if ($Attempt.fingerprint -eq $previous[-1].output_fingerprint -or $Attempt.fingerprint -eq $previous[-1].fingerprint) {
                throw 'Inputs are unchanged since the failed paid attempt. Repair or change the diagnosed execution settings before retrying.'
            }
        }
    }
    $State.attempts = @($State.attempts) + @($Attempt)
}

function Complete-CampaignAttempt($Attempt, $Scorecard, [int]$ExitCode, [bool]$TimedOut) {
    if ($Attempt.Contains('effective_constraints')) {
        return Complete-ObservedCampaignAttempt $Attempt $Scorecard $ExitCode $TimedOut
    }
    # Historical finite attempt reconciliation preserves its original contract.
    if ($Attempt.status -eq 'billing-quarantined') { throw 'A permanent billing quarantine cannot release its retained allocation.' }
    $Attempt.exit_code = $ExitCode
    $Attempt.timed_out = $TimedOut
    $Attempt.finished = [DateTimeOffset]::UtcNow.ToString('o')
    $Attempt.status = 'unresolved'
    $Attempt.verdict = 'incomplete'
    if ($null -eq $Scorecard) {
        if ($Attempt.mode -eq 'DryRun') { $Attempt.status = 'accounted' }
        return
    }
    $expected = if ($Attempt.scenario -eq 'A') { 'a-vue-taskboard' } else { 'b-aspnet-inventory' }
    if ($Attempt.mode -eq 'Repair') { $expected = $Attempt.scenario.ToLowerInvariant() + '-targeted-repair' }
    if ($Scorecard.schema -ne 'vcp-practical-scenario/1' -or $Scorecard.scenario -ne $expected -or
        [bool]$Scorecard.dry_run -ne ($Attempt.mode -eq 'DryRun') -or
        [decimal]$Scorecard.max_scenario_usd -ne [decimal]$Attempt.cap_usd -or
        $Scorecard.workspace -ine $Attempt.project -or $Scorecard.vcp -ine $Attempt.executable) { throw 'Scorecard does not match the reserved attempt.' }
    $Attempt.verdict = [string]$Scorecard.verdict
    if ($Attempt.mode -eq 'Repair') {
        $Attempt.verdict = if ($Scorecard.verdict -eq 'pass' -and $Scorecard.spend_evidence_complete -eq $true -and
            -not $Scorecard.paid_execution_block -and $ExitCode -eq 0 -and -not $TimedOut) { 'repair-pass' } else { 'repair-fail' }
    }
    $Attempt.paid_execution_block = $Scorecard.paid_execution_block
    $Attempt.failed_gates = @($Scorecard.gates | Where-Object { $_.required -and $_.outcome -eq 'fail' } | Select-Object stage, id, detail)
    if ($Attempt.mode -in 'Full', 'Repair' -and $Scorecard.spend_evidence_complete -eq $true -and -not $TimedOut) {
        if (-not $Scorecard.Contains('spend_usd') -or $null -eq $Scorecard.spend_usd) { throw 'Scorecard is missing spend evidence.' }
        $observed = [decimal]$Scorecard.spend_usd
        if ($observed -lt 0 -or $observed -gt [decimal]$Attempt.cap_usd) { throw 'Reported spend exceeds the reserved attempt cap or is invalid.' }
        $Attempt.liability_usd = $observed
        $Attempt.status = 'accounted'
    }
    elseif ($Attempt.mode -eq 'DryRun' -and -not $TimedOut) { $Attempt.status = 'accounted' }
}

function Get-CampaignObservations($State) {
    $observed = [decimal]0; $unknown = 0
    foreach ($attempt in $State.attempts) {
        if ($attempt.Contains('observed_spend_usd') -and $null -ne $attempt.observed_spend_usd) {
            $observed += [decimal]$attempt.observed_spend_usd
        }
        elseif ($attempt.status -eq 'accounted') { $observed += [decimal]$attempt.liability_usd }
        if (-not $attempt.Contains('spend_evidence_complete') -or -not $attempt.spend_evidence_complete) {
            if ($attempt.status -ne 'accounted') { $unknown++ }
        }
    }
    return @{ observed_spend_usd = $observed; attempts_with_unknown_spend = $unknown; total_spend_known = ($unknown -eq 0) }
}

function Complete-ObservedCampaignAttempt($Attempt, $Scorecard, [int]$ExitCode, [bool]$TimedOut) {
    $Attempt.exit_code = $ExitCode; $Attempt.timed_out = $TimedOut
    $Attempt.finished = [DateTimeOffset]::UtcNow.ToString('o')
    $Attempt.status = 'unresolved'; $Attempt.verdict = 'incomplete'
    $Attempt.spend_evidence_complete = $false
    if ($null -eq $Scorecard) { return }
    $expected = if ($Attempt.mode -eq 'Repair') { $Attempt.scenario.ToLowerInvariant() + '-targeted-repair' }
        elseif ($Attempt.scenario -eq 'A') { 'a-vue-taskboard' } else { 'b-aspnet-inventory' }
    if ($Scorecard.schema -ne 'vcp-practical-scenario/1' -or $Scorecard.scenario -ne $expected -or
        [bool]$Scorecard.dry_run -ne ($Attempt.mode -eq 'DryRun') -or
        $Scorecard.workspace -ine $Attempt.project -or $Scorecard.vcp -ine $Attempt.executable -or
        -not $Scorecard.Contains('effective_constraints') -or $Scorecard.effective_constraints.spend -ne 'unbounded') {
        throw 'Scorecard identity or effective policy does not match this execution attempt.'
    }
    if (-not $Scorecard.Contains('spend_usd') -or $null -eq $Scorecard.spend_usd -or [decimal]$Scorecard.spend_usd -lt 0) {
        throw 'Scorecard is missing valid observed spend evidence.'
    }
    $Attempt.observed_spend_usd = [decimal]$Scorecard.spend_usd
    $Attempt.spend_evidence_complete = $Scorecard.spend_evidence_complete -eq $true
    $Attempt.liability_usd = if ($Attempt.spend_evidence_complete) { $Attempt.observed_spend_usd } else { $null }
    $Attempt.paid_execution_block = $Scorecard.paid_execution_block
    $Attempt.failed_gates = @($Scorecard.gates | Where-Object { $_.required -and $_.outcome -eq 'fail' } | Select-Object stage, id, detail)
    $uncertain = @($Attempt.failed_gates | Where-Object { $_.id -eq 'jsonl' -or $_.id -like 'inspect-*' -or $_.id -eq 'dispatch-evidence' })
    $scoped = $true
    if ($Attempt.mode -ne 'DryRun') {
        if (-not $Scorecard.Contains('stages')) { $scoped = $false }
        else {
            $launched = @($Scorecard.stages | Where-Object { -not $_.skipped })
            if (-not $launched.Count) { $scoped = $false }
            foreach ($stage in $launched) {
                $proof = @($Scorecard.gates | Where-Object { $_.stage -eq $stage.stage -and $_.id -eq 'jsonl' -and $_.required -and $_.outcome -eq 'pass' })
                if (-not $stage.task -or -not $stage.session -or $null -eq $stage.exit_code -or $proof.Count -ne 1) { $scoped = $false }
            }
        }
    }
    if ($TimedOut -or $Scorecard.paid_execution_block -or $uncertain.Count -gt 0 -or -not $scoped) {
        $Attempt.spend_evidence_complete = $false
        $Attempt.liability_usd = $null
        return
    }
    $Attempt.status = if ($Attempt.spend_evidence_complete) { 'accounted' } else { 'observed' }
    $passed = $Scorecard.verdict -in 'pass', 'dry-run-pass' -and $ExitCode -eq 0 -and $Attempt.failed_gates.Count -eq 0
    $Attempt.verdict = if ($Attempt.mode -eq 'Repair') { if ($passed) { 'repair-pass' } else { 'repair-fail' } }
        elseif ($passed) { $Scorecard.verdict } else { 'fail' }
}

function Assert-CampaignBillingQuarantine($Attempt, $Scorecard, [object[]]$Bundles, [string]$Reason, [string]$ScenarioRoot = '') {
    if (-not $Reason -or $Attempt.status -ne 'unresolved' -or $Attempt.mode -ne 'Full' -or
        $null -eq $Attempt.exit_code -or $Attempt.timed_out -or [decimal]$Attempt.liability_usd -ne [decimal]$Attempt.cap_usd) {
        throw 'Billing quarantine requires an explained, exited, unresolved Full attempt with its entire cap still reserved.'
    }
    # Reuse scorecard identity checks without changing the retained attempt.
    $copy = @{}; foreach ($key in $Attempt.Keys) { $copy[$key] = $Attempt[$key] }
    Complete-CampaignAttempt $copy $Scorecard $Attempt.exit_code $false
    if ($Scorecard.spend_evidence_complete -ne $false -or $Scorecard.verdict -ne 'fail') { throw 'Only failed attempts with unresolved billing can be quarantined.' }
    $stages = @($Scorecard.stages | Where-Object { -not $_.skipped })
    if (-not $stages.Count -or @($stages | Where-Object { -not $_.task -or $_.exit_code -notin 0, 7 }).Count) { throw 'Every launched stage must have a scoped terminal result.' }
    $tasks = @($stages.task | Sort-Object -Unique)
    if ($tasks.Count -ne $Bundles.Count) { throw 'Complete canonical evidence is required for every launched task.' }
    $seen = @{}; $totalUnresolved = [decimal]0; $totalLiability = [decimal]0; $processProofs = @()
    foreach ($bundle in $Bundles) {
        $task = [string]$bundle.task.scope.task
        $agentPages = @($bundle.agents)
        if ($bundle.kind -ne 'inspection_bundle' -or $bundle.schema_version -ne 1 -or $task -notin $tasks -or $seen.ContainsKey($task) -or
            $bundle.task.state -notin 'paused', 'completed' -or $agentPages.Count -ne 1 -or
            $agentPages[0].root -ne $task -or $agentPages[0].total -ne 0 -or @($agentPages[0].items).Count -ne 0 -or $agentPages[0].next_offset) {
            throw 'Task must be durably stopped with no active agents or missing scope.'
        }
        # task.editing describes editing authority, not liveness. Canonical
        # effects, agents, task state and the OS process check establish quiescence.
        $seen[$task] = $true
        $views = @{}
        foreach ($name in 'costs', 'tools') {
            $pages = @($bundle.views[$name])
            if (-not $pages.Count -or $pages[-1].next_cursor) { throw 'Canonical cost/tool evidence has missing pages.' }
            $views[$name] = @()
            foreach ($page in $pages) {
                if ($page.view -ne $name -or $page.scope.task -ne $task -or $page.source_watermark -ne $bundle.source_watermark -or
                    @($page.gaps).Count -ne 0 -or @($page.items | Where-Object { $_.visibility -ne 'available' }).Count) {
                    throw 'Canonical cost/tool evidence is incomplete, hidden, or from inconsistent snapshots.'
                }
                $views[$name] += @($page.items)
            }
        }
        foreach ($effect in @($views.tools | Where-Object { $_.collection -eq 'effect' })) {
            if ($effect.record.scope.task -ne $task) { throw 'Tool effect scope differs from quarantined task.' }
            if ($effect.record.state -ne 'succeeded') {
                $processProofs += Assert-CampaignFailedProcess $effect.record $bundle $ScenarioRoot $Attempt.project
            }
        }
        $items = @($views.costs)
        $ledgers = @($items | Where-Object { $_.collection -eq 'ledger' -and $_.record.scope.task -eq $task })
        if ($ledgers.Count -ne 1) { throw 'Exactly one scoped canonical ledger is required per task.' }
        $ledger = $ledgers[0].record
        if ($ledger.currency -ne 'USD' -or [decimal]$ledger.active -ne 0 -or $ledger.overrun -ne $false -or
            [decimal]$ledger.unresolved -lt 0 -or [decimal]$ledger.settled -lt 0 -or
            [decimal]$ledger.settled + [decimal]$ledger.unresolved -gt [decimal]$ledger.cap -or
            [decimal]$ledger.cap / 1000000 -gt [decimal]$Attempt.cap_usd) { throw 'Ledger has active, invalid, or over-cap liability.' }
        $pending = @($items | Where-Object { $_.collection -eq 'reservation' -and $_.record.phase -eq 'reconciliation_pending' })
        $unresolved = [decimal]0
        foreach ($reservation in $pending) {
            $matches = @($items | Where-Object { $_.collection -eq 'attempt' -and $_.id -eq $reservation.record.attempt })
            if ($reservation.record.scope.task -ne $task -or $matches.Count -ne 1 -or $matches[0].record.scope.task -ne $task -or
                $matches[0].record.phase -ne 'reconciliation_pending' -or [decimal]$reservation.record.liability -le 0) {
                throw 'Unresolved liability must map completely to canonical provider attempts.'
            }
            $unresolved += [decimal]$reservation.record.liability
        }
        if ($unresolved -ne [decimal]$ledger.unresolved) { throw 'Unresolved ledger liability is not fully explained by provider reservations.' }
        $totalUnresolved += $unresolved
        $totalLiability += [decimal]$ledger.settled + $unresolved
    }
    if ($totalUnresolved -le 0) { throw 'No unresolved provider billing was proved.' }
    if ($totalLiability / 1000000 -gt [decimal]$Attempt.cap_usd) { throw 'Combined task liabilities exceed the permanently retained attempt allocation.' }
    return @{ schema = 'vcp-ab-billing-quarantine/1'; attempt = $Attempt.id; at = [DateTimeOffset]::UtcNow.ToString('o')
        reason = $Reason; tasks = $tasks; unresolved_provider_usd = $totalUnresolved / 1000000
        permanent_hold_usd = [decimal]$Attempt.cap_usd; resume_forbidden = $true; workspace_reuse_forbidden = $true
        terminal_processes = $processProofs }
}

function Assert-CampaignQuarantineBound($Attempt, $Inventory, [object[]]$OriginalBundles, [object[]]$FreshBundles, [string]$ScenarioRoot = '') {
    if ($Attempt.status -ne 'billing-quarantined' -or $Attempt.mode -ne 'Full' -or $Attempt.timed_out -or $null -eq $Attempt.exit_code -or
        $Attempt.ContainsKey('bounded_liability') -or [decimal]$Attempt.liability_usd -ne [decimal]$Attempt.cap_usd -or
        -not $Inventory.tasks.Count -or $OriginalBundles.Count -ne $Inventory.tasks.Count -or $FreshBundles.Count -ne $Inventory.tasks.Count) {
        throw 'Only an exited, fully reserved original billing quarantine can receive a first bounded-cap assessment.'
    }
    $total = [decimal]0; $roots = @(); $seen = @{}; $processProofs = @()
    foreach ($scope in $Inventory.tasks) {
        $task = [string]$scope.task
        if ($seen.ContainsKey($task)) { throw 'Duplicate original root scope.' }; $seen[$task] = $true
        $original = @($OriginalBundles | Where-Object { $_.task.scope.task -eq $task })
        $fresh = @($FreshBundles | Where-Object { $_.task.scope.task -eq $task })
        if ($original.Count -ne 1 -or $fresh.Count -ne 1) { throw 'Each root requires exactly one original and fresh canonical bundle.' }
        $originalWatermark = [decimal]0
        foreach ($bundle in @($original[0], $fresh[0])) {
            if ($bundle.kind -ne 'inspection_bundle' -or $bundle.schema_version -ne 1 -or [string]$bundle.source_watermark -notmatch '^\d+$' -or
                $bundle.task.state -notin 'paused', 'completed' -or $bundle.task.parent -or $bundle.task.root -ne $task -or
                [decimal]$bundle.source_watermark -lt $originalWatermark) { throw 'Root identity, quiescence or snapshot freshness is invalid.' }
            $originalWatermark = [decimal]$bundle.source_watermark
            foreach ($field in 'task', 'session', 'workspace') { if (-not $scope[$field] -or $bundle.task.scope[$field] -ne $scope[$field]) { throw 'Task scope differs from original dispatch.' } }
            $agents = @($bundle.agents)
            if ($agents.Count -ne 1 -or $agents[0].root -ne $task -or $agents[0].total -ne 0 -or @($agents[0].items).Count -ne 0 -or $agents[0].next_offset) { throw 'Agents are active or incompletely observed.' }
            foreach ($name in 'costs', 'tools', 'verification', 'routing', 'policy', 'outputs') {
                $pages = @($bundle.views[$name])
                if (-not $pages.Count -or $pages[-1].next_cursor) { throw 'Canonical inspection pages are incomplete.' }
                $ids = @{}
                foreach ($page in $pages) {
                    $gaps = @($page.gaps)
                    # Fixed-provider tasks explicitly report absent automatic
                    # routing decisions. This does not hide accounting facts.
                    $fixedRouting = $name -eq 'routing' -and $gaps.Count -eq 1 -and
                        $gaps[0].reason -ceq 'no automatic routing decision retained for this task; fixed provider or no admitted routed request' -and
                        $gaps[0].visibility -eq 'unavailable'
                    if ($page.view -ne $name -or $page.source_watermark -ne $bundle.source_watermark -or ($gaps.Count -and -not $fixedRouting)) { throw 'Canonical inspection pages are inconsistent.' }
                    foreach ($field in 'task', 'session', 'workspace') { if ($page.scope[$field] -ne $scope[$field]) { throw 'Page scope differs from dispatch.' } }
                    foreach ($item in @($page.items)) {
                        $key = "$($item.collection):$($item.id)"
                        if (-not $item.id -or $item.visibility -ne 'available' -or $ids.ContainsKey($key)) { throw 'Canonical items are hidden, unidentified or duplicated.' }
                        $ids[$key] = $true
                    }
                }
            }
            foreach ($effect in @($bundle.views.tools | ForEach-Object items | Where-Object collection -eq 'effect')) {
                foreach ($field in 'task', 'session', 'workspace') { if ($effect.record.scope[$field] -ne $scope[$field]) { throw 'Tool scope differs from root.' } }
                if ($effect.record.state -ne 'succeeded') {
                    $processProofs += Assert-CampaignFailedProcess $effect.record $bundle $ScenarioRoot $Attempt.project
                }
            }
            $items = @($bundle.views.costs | ForEach-Object items)
            foreach ($item in $items) {
                foreach ($field in 'task', 'session', 'workspace') { if ($item.record.scope[$field] -ne $scope[$field]) { throw 'Accounting scope differs from root.' } }
            }
            $ledgers = @($items | Where-Object collection -eq 'ledger')
            if ($ledgers.Count -ne 1 -or $ledgers[0].id -ne $task) { throw 'Exactly one original root ledger is required.' }
            $ledger = $ledgers[0].record
            foreach ($field in 'cap', 'settled', 'active', 'unresolved') { if ([string]$ledger[$field] -notmatch '^\d+$') { throw 'Malformed canonical amount.' } }
            if ($ledger.currency -ne 'USD' -or $ledger.overrun -ne $false -or [decimal]$ledger.active -ne 0 -or $ledger.allocations.Count -ne 0 -or
                [decimal]$ledger.cap -ne [decimal]$scope.original_cap_usd * 1000000 -or [decimal]$ledger.cap -le 0 -or
                [decimal]$ledger.settled + [decimal]$ledger.unresolved -gt [decimal]$ledger.cap) { throw 'Original cap, overrun, active liability or child allocation is invalid.' }
            $attempts = @($items | Where-Object collection -eq 'attempt'); $reservations = @($items | Where-Object collection -eq 'reservation')
            if ($attempts.Count -ne $reservations.Count) { throw 'Every provider attempt requires exactly one reservation.' }
            $charged = [decimal]0; $unresolved = [decimal]0
            foreach ($provider in $attempts) {
                $a = $provider.record
                $reservation = @($reservations | Where-Object { $_.id -eq $a.reservation -and $_.record.attempt -eq $provider.id })
                if ($provider.id -ne $a.id -or $a.root -ne $task -or $reservation.Count -ne 1 -or $a.phase -notin 'settled', 'released', 'reconciliation_pending') { throw 'Provider attempt is active, unsupported or not exactly reserved.' }
                $r = $reservation[0].record
                foreach ($value in @($a.charged, $r.charged, $r.liability, $r.amount.micros, $a.quote.amount.micros)) { if ([string]$value -notmatch '^\d+$') { throw 'Malformed provider amount.' } }
                if ($r.id -ne $reservation[0].id -or $r.root -ne $task -or $r.phase -ne $a.phase -or $r.amount.currency -ne 'USD' -or $a.quote.amount.currency -ne 'USD' -or
                    [decimal]$r.amount.micros -ne [decimal]$a.quote.amount.micros -or [decimal]$r.charged -ne [decimal]$a.charged -or
                    [decimal]$r.liability -gt [decimal]$r.amount.micros -or
                    ($a.phase -eq 'reconciliation_pending' -and [decimal]$r.liability -le 0) -or
                    ($a.phase -ne 'reconciliation_pending' -and [decimal]$r.liability -ne 0)) { throw 'Liability is inconsistent or a zero-priced unknown attempt is unbounded.' }
                $charged += [decimal]$r.charged; $unresolved += [decimal]$r.liability
            }
            if ($charged -ne [decimal]$ledger.settled -or $unresolved -ne [decimal]$ledger.unresolved) { throw 'Reservations do not completely explain the ledger.' }
        }
        $total += [decimal]$scope.original_cap_usd
        $roots += @{ scope = $scope; original_cap_usd = [decimal]$scope.original_cap_usd; original_watermark = $original[0].source_watermark; fresh_watermark = $fresh[0].source_watermark }
    }
    if ($total -le 0 -or $total -ge [decimal]$Attempt.liability_usd) { throw 'Original root caps do not prove a strict smaller positive allocation.' }
    return @{ schema = 'vcp-ab-bounded-quarantine/1'; attempt = $Attempt.id; original_attempt_cap_usd = [decimal]$Attempt.cap_usd
        prior_allocation_usd = [decimal]$Attempt.liability_usd; bounded_liability_usd = $total; roots = $roots
        accounting_complete = $false; resume_forbidden = $true; workspace_reuse_forbidden = $true; terminal_processes = $processProofs }
}

function Assert-NoCampaignNativeProcess([string]$ScenarioRoot, [object[]]$Processes) {
    $normalizedRoot = $ScenarioRoot.Replace('/', '\')
    foreach ($process in $Processes) {
        if ($process.Name -ieq 'vcp.exe' -and [string]::IsNullOrWhiteSpace([string]$process.CommandLine)) { throw 'Cannot prove a VCP process is unrelated because its command line is unavailable.' }
        if ($process.CommandLine -and ([string]$process.CommandLine).Replace('/', '\').IndexOf($normalizedRoot, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
            throw 'A native process still references the quarantined scenario data; wait for it to exit.'
        }
    }
}

Export-ModuleMember -Function Write-CampaignState, Get-CampaignLiability, Get-CampaignObservations, Add-CampaignAttempt, Complete-CampaignAttempt, Open-CampaignLock, Get-CampaignProjectLock, Assert-CampaignActiveAttempts, Assert-CampaignBillingQuarantine, Assert-CampaignQuarantineBound, Assert-NoCampaignNativeProcess
