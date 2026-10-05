# SPDX-License-Identifier: Apache-2.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -DisableNameChecking
Import-Module (Join-Path $PSScriptRoot 'VcpCampaignProcessProof.psm1') -DisableNameChecking

function Get-RecoveryCommand([string[]]$Arguments) {
    $index = 0
    while ($index -lt $Arguments.Count) {
        if ($Arguments[$index] -in '--format', '--workspace', '--data-dir', '--config') { $index += 2; continue }
        if ($Arguments[$index] -eq '--non-interactive') { $index++; continue }
        break
    }
    if ($index -ge $Arguments.Count) { throw 'Command audit has no native command.' }
    $verb = $Arguments[$index]
    if ($verb -in 'tasks', 'sessions') {
        if ($index + 1 -ge $Arguments.Count) { throw 'Command audit has no native subcommand.' }
        $verb += ' ' + $Arguments[$index + 1]
    }
    return $verb
}

function Get-CampaignRecoveryInventory($Attempt, $Card, [string]$ScenarioRoot, [switch]$ForQuarantineBound) {
    $requiredStatus = if ($ForQuarantineBound) { 'billing-quarantined' } else { 'unresolved' }
    if ($Attempt.status -ne $requiredStatus -or $Attempt.mode -ne 'Full' -or $null -eq $Attempt.exit_code -or $Attempt.timed_out) {
        throw 'Fresh accounting requires an exited unresolved Full attempt; permanent quarantines cannot be recovered.'
    }
    $scenarioName = if ($Attempt.scenario -eq 'A') { 'a-vue-taskboard' } else { 'b-aspnet-inventory' }
    if ($Card.schema -ne 'vcp-practical-scenario/1' -or $Card.scenario -ne $scenarioName -or $Card.dry_run -ne $false -or
        $Card.verdict -ne 'fail' -or $Card.spend_evidence_complete -ne $false -or
        $Card.workspace -ine $Attempt.project -or $Card.vcp -ine $Attempt.executable -or
        [decimal]$Card.max_scenario_usd -ne [decimal]$Attempt.cap_usd -or
        (Split-Path $ScenarioRoot -Leaf) -ne $Card.run_id -or [DateTimeOffset]$Card.started -lt [DateTimeOffset]$Attempt.started -or
        [DateTimeOffset]$Card.finished -lt [DateTimeOffset]$Card.started) {
        throw 'The original failed scorecard must match the unresolved attempt.'
    }
    $logs = Join-Path $ScenarioRoot 'logs'
    $auditPath = Join-Path $logs 'vcp-commands.jsonl'
    $textPath = Join-Path $logs 'vcp-commands.log'
    $records = @(Get-Content -LiteralPath $auditPath | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json -AsHashtable -Depth 32 })
    if (-not $records.Count -or $records.Count -gt 2000) { throw 'Missing or unbounded native command audit.' }
    $starts = @([regex]::Matches([IO.File]::ReadAllText($textPath), '(?m)^\S+ \[[^\r\n]+\] START id=([0-9a-f]{32})\r?$') | ForEach-Object { $_.Groups[1].Value })
    $ids = @($records.id)
    if ($starts.Count -ne $ids.Count -or @($ids | Sort-Object -Unique).Count -ne $ids.Count -or
        @($starts | Where-Object { $_ -notin $ids }).Count -or @($ids | Where-Object { $_ -notin $starts }).Count) {
        throw 'Every started native command must have exactly one completion audit before accounting can be released.'
    }
    $tasks = @{}; $files = @($auditPath, $textPath, $Attempt.scorecard)
    foreach ($record in $records) {
        if ($record.workspace -ine $Attempt.project -or $record.executable -ine $Attempt.executable -or -not $record.completed_at) { throw 'Command audit identity or completion is incomplete.' }
        $verb = Get-RecoveryCommand @($record.argv)
        if ($ForQuarantineBound -and $verb -notin '--version', 'inspect-bundle', 'doctor', 'setup', 'skills', 'models', 'inspect', 'run') {
            throw 'Bounded quarantine supports original run roots only; an unclassified or continuation command remains uncertain.'
        }
        if ($ForQuarantineBound -and $verb -eq 'setup') {
            $arguments = @($record.argv); $setupIndex = [array]::IndexOf($arguments, 'setup')
            $tail = @($arguments | Select-Object -Skip ($setupIndex + 1))
            $metadata = $tail.Count -eq 7 -and $tail[0] -eq 'provider-metadata' -and $tail[1] -eq '--model' -and
                $tail[3] -eq '--endpoint' -and $tail[5] -eq '--output' -and $tail[2] -and $tail[4] -and $tail[6]
            if ($metadata) {
                $prefix = [IO.Path]::GetFullPath($ScenarioRoot).TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar
                $stdout = [IO.Path]::GetFullPath($record.stdout_path)
                if (-not $stdout.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or
                    -not [IO.Path]::GetFullPath($tail[6]).StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or
                    $record.accepted -or $record.task -or $record.session -or $record.timed_out -or $record.exit_code -ne 0 -or
                    (Get-Item -LiteralPath $stdout).Length -gt 1MB) { throw 'Provider metadata command identity or completion is invalid.' }
                $parsed = ConvertFrom-JsonLines ([IO.File]::ReadAllText($stdout))
                $frames = @($parsed.Frames)
                if ($parsed.Invalid.Count -or $frames.Count -ne 1 -or $frames[0].type -ne 'result' -or $frames[0].schema_version -ne 1 -or
                    $frames[0].exit_code -ne 0 -or $frames[0].scope -or -not $frames[0].correlation -or $frames[0].data.model_calls -ne 0 -or
                    $frames[0].data.model -ne $tail[2] -or $frames[0].data.endpoint -ne $tail[4] -or $frames[0].data.status -ne 'created') {
                    throw 'Provider metadata result does not prove a complete zero-model-call capture.'
                }
                $files += $stdout
            }
            if (-not $tail.Count -or (-not $metadata -and $tail[0] -ne 'check' -and
                -not ($tail.Count -ge 2 -and (($tail[0] -eq 'credential' -and $tail[1] -eq 'status') -or
                    ($tail[0] -eq 'profile' -and $tail[1] -eq '--snapshot'))))) {
                throw 'Unclassified setup command cannot be assumed nonbillable.'
            }
        }
        $paid = $verb -in 'run', 'resume', 'tasks resume', 'sessions fork'
        if (-not $paid -and -not $record.accepted) { continue }
        $stdout = [IO.Path]::GetFullPath($record.stdout_path)
        $prefix = [IO.Path]::GetFullPath($ScenarioRoot).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
        if (-not $stdout.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Get-Item -LiteralPath $stdout).Length -gt 128MB) { throw 'Dispatch evidence path is outside this scenario or exceeds its read bound.' }
        $files += $stdout
        $parsed = ConvertFrom-JsonLines ([IO.File]::ReadAllText($stdout))
        $accepted = @($parsed.Frames | Where-Object type -eq 'accepted')
        $results = @($parsed.Frames | Where-Object type -eq 'result')
        if ($record.timed_out -or $null -eq $record.exit_code -or $parsed.Invalid.Count -ne 0 -or $results.Count -ne 1 -or
            $parsed.Frames[-1].type -ne 'result' -or $results[0].schema_version -ne 1 -or $results[0].exit_code -ne $record.exit_code) { throw 'A dispatch lacks a complete terminal JSONL result.' }
        if (-not $accepted.Count) {
            # Guardrails can reject a run before it starts. Any other unscoped
            # execution remains uncertain and keeps the entire reservation.
            $conditions = @($results[0].conditions.PSObject.Properties | Where-Object Value -eq $true | ForEach-Object Name)
            if ($record.accepted -or $record.task -or $record.session -or $parsed.Frames.Count -ne 1 -or $record.exit_code -ne 2 -or $results[0].scope -or $results[0].receipt -or
                $conditions.Count -ne 1 -or $conditions[0] -ne 'invalid_configuration') { throw 'Unscoped dispatch cannot be assumed to have zero cost.' }
            continue
        }
        if (-not $paid -or $accepted.Count -ne 1 -or $accepted[0].schema_version -ne 1 -or -not $record.accepted) { throw 'Unexpected or duplicate task acceptance in command evidence.' }
        $scope = $accepted[0].scope
        foreach ($field in 'workspace', 'session', 'task') {
            if (-not $scope.$field -or $scope.$field -ne $results[0].scope.$field) { throw 'Accepted/result scope differs.' }
        }
        if (-not $accepted[0].correlation -or $accepted[0].correlation -ne $results[0].correlation -or
            $record.task -ne $scope.task -or $record.session -ne $scope.session) { throw 'Command audit and terminal receipt identities differ.' }
        $dataIndex = [array]::IndexOf(@($record.argv), '--data-dir')
        if ($dataIndex -lt 0 -or [IO.Path]::GetFullPath($record.argv[$dataIndex + 1]) -ine [IO.Path]::GetFullPath((Join-Path $ScenarioRoot 'vcp-data'))) { throw 'Dispatch used an unexpected native data directory.' }
        if ($tasks.ContainsKey($scope.task) -and ($tasks[$scope.task].session -ne $scope.session -or $tasks[$scope.task].workspace -ne $scope.workspace)) { throw 'Repeated task scope is inconsistent.' }
        $entry = @{ task = $scope.task; session = $scope.session; workspace = $scope.workspace }
        if ($ForQuarantineBound) {
            $arguments = @($record.argv)
            $budgetIndex = [array]::IndexOf($arguments, '--budget-usd')
            if ($tasks.ContainsKey($scope.task) -or @($arguments | Where-Object { $_ -eq '--budget-usd' }).Count -ne 1 -or
                $budgetIndex -lt 0 -or $budgetIndex + 1 -ge $arguments.Count -or $arguments[$budgetIndex + 1] -notmatch '^\d+(?:\.\d{1,6})?$') {
                throw 'Each dispatched root requires one original explicit native budget.'
            }
            $entry.original_cap_usd = [decimal]::Parse($arguments[$budgetIndex + 1], [Globalization.CultureInfo]::InvariantCulture)
            if ($entry.original_cap_usd -le 0 -or $entry.original_cap_usd -gt [decimal]$Attempt.cap_usd) { throw 'Original root cap is invalid.' }
        }
        $tasks[$scope.task] = $entry
    }
    if (-not $tasks.Count) { throw 'No fully scoped paid task was proved for fresh accounting recovery.' }
    foreach ($stage in @($Card.stages | Where-Object { -not $_.skipped })) {
        if (-not $stage.task -or -not $tasks.ContainsKey($stage.task)) { throw 'Scorecard contains a launched stage missing from the complete dispatch inventory.' }
    }
    return @{ tasks = @($tasks.Values | Sort-Object task); files = @($files | Sort-Object -Unique) }
}

function Get-CampaignRecoveredCost($Attempt, $Inventory, [object[]]$Bundles) {
    if ($Attempt.status -ne 'unresolved' -or $Bundles.Count -ne $Inventory.tasks.Count) { throw 'Every scoped task requires fresh canonical evidence; permanent quarantines cannot be recovered.' }
    $seen = @{}; $total = [decimal]0
    foreach ($bundle in $Bundles) {
        $task = [string]$bundle.task.scope.task
        $scope = @($Inventory.tasks | Where-Object task -eq $task)
        $agents = @($bundle.agents)
        if ($bundle.kind -ne 'inspection_bundle' -or $bundle.schema_version -ne 1 -or -not $bundle.source_watermark -or
            $scope.Count -ne 1 -or $seen.ContainsKey($task) -or $bundle.task.state -notin 'paused', 'completed', 'failed', 'cancelled' -or
            $agents.Count -ne 1 -or $agents[0].root -ne $task -or $agents[0].total -ne 0 -or @($agents[0].items).Count -ne 0 -or $agents[0].next_offset) { throw 'Recovered task is live, ambiguous, or has active agents.' }
        $seen[$task] = $true
        foreach ($field in 'session', 'workspace') { if ($bundle.task.scope.$field -ne $scope[0].$field) { throw 'Recovered task scope differs from original acceptance.' } }
        foreach ($name in 'costs', 'tools') {
            $pages = @($bundle.views.$name)
            if (-not $pages.Count -or $pages[-1].next_cursor) { throw 'Fresh canonical inspection has incomplete pages.' }
            foreach ($page in $pages) {
                if ($page.view -ne $name -or $page.source_watermark -ne $bundle.source_watermark -or @($page.gaps).Count -ne 0 -or
                    @($page.items | Where-Object { $_.visibility -ne 'available' }).Count) { throw 'Fresh canonical inspection is inconsistent or hidden.' }
                foreach ($field in 'task', 'session', 'workspace') { if ($page.scope.$field -ne $scope[0].$field) { throw 'Fresh inspection page scope differs from acceptance.' } }
            }
        }
        foreach ($effect in @(Get-InspectItems $bundle.views.tools | Where-Object collection -eq 'effect')) {
            if ($effect.record.scope.task -ne $task -or $effect.record.state -notin 'succeeded', 'failed', 'cancelled') { throw 'A tool effect is still active or its outcome is unknown.' }
        }
        $ledgers = @(Get-InspectItems $bundle.views.costs | Where-Object collection -eq 'ledger')
        if ($ledgers.Count -ne 1 -or $ledgers[0].record.scope.task -ne $task -or $ledgers[0].record.currency -ne 'USD' -or
            $ledgers[0].record.overrun -ne $false -or [string]$ledgers[0].record.cap -notmatch '^\d+$') { throw 'Fresh task ledger identity, currency, cap or overrun is invalid.' }
        $cost = Get-VcpTaskCost $bundle.views.costs
        if ($null -eq $cost.Usd -or $cost.Usd -gt [decimal]$ledgers[0].record.cap / 1000000) { throw 'Fresh task ledger still has active/unresolved liability or exceeds its cap.' }
        $total += $cost.Usd
    }
    if ($total -gt [decimal]$Attempt.cap_usd) { throw 'Recovered aggregate settlement exceeds the retained campaign allocation.' }
    return $total
}

function Invoke-CampaignAccountingRecovery($Attempt, $Card, [string]$ScenarioRoot, [string]$Executable) {
    $inventory = Get-CampaignRecoveryInventory $Attempt $Card $ScenarioRoot
    $Executable = (Resolve-Path -LiteralPath $Executable).Path
    $root = Join-Path $Attempt.root ('accounting-recovery-' + [guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($root)
    $ctx = @{ Name = 'accounting-recovery'; Vcp = $Executable; Workspace = $Attempt.project; Data = Join-Path $ScenarioRoot 'vcp-data'
        Logs = $root; Results = $root; ProgressLog = Join-Path $root 'progress.log'; SkipPaidStages = $true }
    $version = Invoke-Vcp $ctx 'metadata' 'version' @('--version') -NoGlobals -DenyProviderCredentials
    if ($version.ExitCode -ne 0 -or $version.TimedOut) { throw 'Recovery executable version check failed.' }
    $versionText = ([IO.File]::ReadAllText($version.StdoutPath)).Trim()
    if ($versionText -notmatch '^vcp \d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?$') { throw 'Recovery executable did not identify a VCP candidate version.' }
    $bundles = @(); $files = @($inventory.files)
    foreach ($scope in $inventory.tasks) {
        # No run/resume/fork, no provider credential, no changes to old evidence.
        $run = Invoke-Vcp $ctx 'metadata' ('inspect-' + $scope.task) @('inspect-bundle', $scope.task) -TimeoutSeconds 1800 -DenyProviderCredentials
        if ($run.ExitCode -ne 0 -or $run.TimedOut -or $run.InvalidLines -ne 0 -or $run.Accepted -or -not $run.Result -or
            @($run.Frames | Where-Object type -eq result).Count -ne 1 -or $run.Frames[-1].type -ne 'result') { throw 'Fresh read-only canonical inspection failed; retain the full allocation.' }
        $bundles += $run.Result.data
        $files += $run.StdoutPath
    }
    $settled = Get-CampaignRecoveredCost $Attempt $inventory $bundles
    return @{ schema = 'vcp-ab-accounting-recovery/1'; at = [DateTimeOffset]::UtcNow.ToString('o'); attempt = $Attempt.id
        root = $root; executable = $Executable; executable_sha256 = Get-Sha256 $Executable
        executable_version = $versionText; settled_usd = $settled
        prior_allocation_usd = $Attempt.liability_usd; original_verdict = $Attempt.verdict; tasks = $inventory.tasks
        evidence_files = @($files | Sort-Object -Unique | ForEach-Object { @{ path = $_; sha256 = Get-Sha256 $_ } }) }
}

function Assert-BoundEvidencePath([string]$Root, [string]$Path) {
    $prefix = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $full = [IO.Path]::GetFullPath($Path)
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $full -PathType Leaf)) { throw 'Bound evidence is missing or outside its original attempt.' }
    $node = Get-Item -LiteralPath $full
    while ($node) {
        if ($node.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Bound evidence cannot traverse a reparse point.' }
        $node = if ($node -is [IO.FileInfo]) { $node.Directory } else { $node.Parent }
    }
    return $full
}

function Get-CampaignQuarantineInputs($Attempt, $Card, [string]$ScenarioRoot) {
    if ($Attempt.status -ne 'billing-quarantined' -or $Attempt.ContainsKey('bounded_liability') -or
        $Attempt.quarantine.permanent_hold_usd -ne $Attempt.cap_usd -or $Attempt.liability_usd -ne $Attempt.cap_usd) { throw 'Original full quarantine allocation is required.' }
    $proofPath = Assert-BoundEvidencePath $Attempt.root $Attempt.quarantine.evidence
    if ((Get-Sha256 $proofPath) -ne $Attempt.quarantine.sha256) { throw 'Original quarantine evidence hash changed.' }
    $proof = Get-Content -LiteralPath $proofPath -Raw | ConvertFrom-Json -AsHashtable -Depth 100
    if ($proof.schema -ne 'vcp-ab-billing-quarantine/1' -or $proof.attempt -ne $Attempt.id -or $proof.permanent_hold_usd -ne $Attempt.cap_usd -or
        $proof.resume_forbidden -ne $true -or $proof.workspace_reuse_forbidden -ne $true -or $proof.native_processes_referencing_data -ne 0) { throw 'Original quarantine identity or isolation evidence is invalid.' }
    if ($proof.ContainsKey('terminal_processes')) {
        foreach ($process in $proof.terminal_processes) { Assert-CampaignProcessProofFiles $process $ScenarioRoot $Attempt.project }
    }
    $inventory = Get-CampaignRecoveryInventory $Attempt $Card $ScenarioRoot -ForQuarantineBound
    if (@($proof.tasks).Count -ne $inventory.tasks.Count -or @($proof.tasks | Where-Object { $_ -notin $inventory.tasks.task }).Count) { throw 'Quarantine task inventory differs from complete native dispatch inventory.' }
    $files = @($inventory.files) + @($proofPath); $original = @(); $seen = @{}
    foreach ($source in $proof.evidence_files) {
        $path = Assert-BoundEvidencePath $Attempt.root $source.path
        if ($seen.ContainsKey($path) -or (Get-Sha256 $path) -ne $source.sha256) { throw 'Original quarantine source changed or is duplicated.' }; $seen[$path] = $true
        if ($path -ine $Attempt.scorecard) {
            if ((Split-Path $path -Leaf) -ne 'inspection-bundle.json' -or -not $path.StartsWith([IO.Path]::GetFullPath($ScenarioRoot).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected original quarantine source.' }
            $original += Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable -Depth 100
        }
        $files += $path
    }
    if (-not $seen.ContainsKey([IO.Path]::GetFullPath($Attempt.scorecard)) -or $original.Count -ne $inventory.tasks.Count) { throw 'Original scorecard or task bundles are missing from hashed quarantine evidence.' }
    # Account for every other retained native audit too. Launcher setup can only
    # check credentials; earlier failed bound inspections can only read metadata.
    $mainAudit = [IO.Path]::GetFullPath((Join-Path $ScenarioRoot 'logs/vcp-commands.jsonl'))
    foreach ($log in @(Get-ChildItem -LiteralPath $Attempt.root -Recurse -File -Filter 'vcp-commands.log')) {
        if (-not (Test-Path -LiteralPath (Join-Path $log.DirectoryName 'vcp-commands.jsonl') -PathType Leaf)) { throw 'A native command start log lacks completion evidence.' }
    }
    foreach ($audit in @(Get-ChildItem -LiteralPath $Attempt.root -Recurse -File -Filter 'vcp-commands.jsonl')) {
        if ($audit.FullName -ieq $mainAudit) { continue }
        $textPath = Join-Path $audit.DirectoryName 'vcp-commands.log'
        $records = @(Get-Content -LiteralPath $audit.FullName | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json -AsHashtable -Depth 32 })
        $starts = @([regex]::Matches([IO.File]::ReadAllText($textPath), '(?m)^\S+ \[[^\r\n]+\] START id=([0-9a-f]{32})\r?$') | ForEach-Object { $_.Groups[1].Value })
        if (-not $records.Count -or $records.Count -gt 2000 -or $starts.Count -ne $records.Count -or
            @($records.id | Sort-Object -Unique).Count -ne $records.Count -or @($starts | Sort-Object -Unique).Count -ne $starts.Count -or
            @($starts | Where-Object { $_ -notin $records.id }).Count -or @($records.id | Where-Object { $_ -notin $starts }).Count) { throw 'Auxiliary native command audit is incomplete or ambiguous.' }
        foreach ($record in $records) {
            $verb = Get-RecoveryCommand @($record.argv)
            $setup = [array]::IndexOf(@($record.argv), 'setup')
            $credentialStatus = $verb -eq 'setup' -and $setup -ge 0 -and @($record.argv).Count -eq $setup + 3 -and
                $record.argv[$setup + 1] -eq 'credential' -and $record.argv[$setup + 2] -eq 'status'
            if ($record.workspace -ine $Attempt.project -or -not $record.completed_at -or $record.accepted -or $record.task -or $record.session -or
                (-not $credentialStatus -and $verb -notin '--version', 'inspect-bundle')) { throw 'An auxiliary or unscoped native command could have dispatched paid work.' }
            $files += Assert-BoundEvidencePath $Attempt.root $record.stdout_path
        }
        $files += @($audit.FullName, $textPath)
    }
    foreach ($path in $files) { [void](Assert-BoundEvidencePath $Attempt.root $path) }
    return @{ inventory = $inventory; original = $original; proof = $proofPath
        hashes = @($files | Sort-Object -Unique | ForEach-Object { @{ path = $_; sha256 = Get-Sha256 $_ } }) }
}

function Invoke-CampaignQuarantineBound($Attempt, $Card, [string]$ScenarioRoot, [string]$Executable, [string]$Reason) {
    if ([string]::IsNullOrWhiteSpace($Reason)) { throw 'BoundQuarantine requires an evidence-based RepairNote.' }
    $inputs = Get-CampaignQuarantineInputs $Attempt $Card $ScenarioRoot
    $Executable = (Resolve-Path -LiteralPath $Executable).Path
    $executableHash = Get-Sha256 $Executable
    $root = Join-Path $Attempt.root ('bounded-quarantine-' + [guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($root)
    $ctx = @{ Name = 'bounded-quarantine'; Vcp = $Executable; Workspace = $Attempt.project; Data = Join-Path $ScenarioRoot 'vcp-data'
        Logs = $root; Results = $root; ProgressLog = Join-Path $root 'progress.log'; SkipPaidStages = $true }
    $version = Invoke-Vcp $ctx 'metadata' 'version' @('--version') -NoGlobals -DenyProviderCredentials
    if ($version.ExitCode -ne 0 -or $version.TimedOut) { throw 'Bound inspection executable version check failed.' }
    $versionText = ([IO.File]::ReadAllText($version.StdoutPath)).Trim()
    if ($versionText -notmatch '^vcp \d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?$') { throw 'Bound inspection executable is not a VCP candidate.' }
    $bundles = @(); $files = @($version.StdoutPath)
    foreach ($scope in $inputs.inventory.tasks) {
        $run = Invoke-Vcp $ctx 'metadata' ('inspect-' + $scope.task) @('inspect-bundle', $scope.task) -TimeoutSeconds 1800 -DenyProviderCredentials
        if ($run.ExitCode -ne 0 -or $run.TimedOut -or $run.InvalidLines -ne 0 -or $run.Accepted -or -not $run.Result -or
            @($run.Frames).Count -ne 1 -or $run.Result.schema_version -ne 1 -or $run.Result.type -ne 'result' -or
            $run.Result.exit_code -ne 0 -or -not $run.Result.correlation) { throw 'Fresh credential-denied inspection did not produce one complete canonical result.' }
        $bundles += $run.Result.data | ConvertTo-Json -Depth 100 | ConvertFrom-Json -AsHashtable -Depth 100
        $files += $run.StdoutPath
    }
    $evidence = Assert-CampaignQuarantineBound $Attempt $inputs.inventory $inputs.original $bundles $ScenarioRoot
    foreach ($source in $inputs.hashes) { if ((Get-Sha256 $source.path) -ne $source.sha256) { throw 'Original evidence changed during inspection; retain full hold.' } }
    if ((Get-Sha256 $Executable) -ne $executableHash) { throw 'Inspection executable changed during assessment.' }
    $evidence.at = [DateTimeOffset]::UtcNow.ToString('o'); $evidence.reason = $Reason; $evidence.root = $root
    $evidence.original_quarantine = @{ path = $inputs.proof; sha256 = Get-Sha256 $inputs.proof }
    $evidence.executable = $Executable; $evidence.executable_sha256 = $executableHash; $evidence.executable_version = $versionText
    $evidence.credential_denied = $true
    $evidence.evidence_files = @($inputs.hashes) + @($files | ForEach-Object { @{ path = $_; sha256 = Get-Sha256 $_ } })
    foreach ($process in $evidence.terminal_processes) { $evidence.evidence_files += @($process.evidence_files) }
    return $evidence
}

Export-ModuleMember -Function Get-CampaignRecoveryInventory, Get-CampaignRecoveredCost, Invoke-CampaignAccountingRecovery, Get-CampaignQuarantineInputs, Invoke-CampaignQuarantineBound
