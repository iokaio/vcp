# SPDX-License-Identifier: Apache-2.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-CampaignRepairSource($State, [string]$SourceAttemptId, [string]$Stage, [string]$Scenario, [string]$Project, [string]$PromptPath) {
    $sources = @($State.attempts | Where-Object id -eq $SourceAttemptId)
    if ($sources.Count -ne 1) { throw 'Repair requires exactly one retained source attempt.' }
    $source = $sources[0]
    if ($source.mode -ne 'Full' -or $source.status -ne 'accounted' -or $source.scenario -ne $Scenario -or
        $source.project -ine $Project -or $source.timed_out -or $null -eq $source.exit_code) {
        throw 'Repair requires an exited, accounted Full attempt in the same scenario and workspace.'
    }
    $cardPath = [IO.Path]::GetFullPath($source.scorecard)
    $root = [IO.Path]::GetFullPath($source.root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $cardPath.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw 'Source scorecard is outside its attempt.' }
    $card = Get-Content -LiteralPath $cardPath -Raw | ConvertFrom-Json -AsHashtable -Depth 100
    $expected = if ($Scenario -eq 'A') { 'a-vue-taskboard' } else { 'b-aspnet-inventory' }
    if ($card.schema -ne 'vcp-practical-scenario/1' -or $card.scenario -ne $expected -or $card.workspace -ine $Project -or $card.dry_run) { throw 'Source scorecard identity mismatch.' }
    if ($card.paid_execution_block -and @($card.paid_execution_block.reasons | Where-Object { $_ -notin 'budget_exhausted', 'durably_paused' }).Count) {
        throw 'Unresolved effects, approval, interruption, or other source stopping conditions prohibit a new repair task.'
    }
    $stages = @($card.stages | Where-Object { $_.stage -eq $Stage -and -not $_.skipped -and $_.task })
    if ($stages.Count -ne 1) { throw 'Repair must name exactly one launched source stage.' }
    $profilePath = [IO.Path]::GetFullPath($stages[0].profile)
    $scenarioRoot = Split-Path (Split-Path $cardPath -Parent) -Parent
    if (-not $profilePath.StartsWith($scenarioRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Source profile is outside its scenario evidence.' }
    $profile = Get-Content -LiteralPath $profilePath -Raw | ConvertFrom-Json -AsHashtable -Depth 100
    if ($profile.workspace -ine $Project -or $profile.version -ne 1 -or $profile.trust_workspace -ne $true -or
        $profile.maximum_autonomy -ne 'autonomous' -or -not @($profile.checks).Count -or -not @($profile.processes).Count -or
        -not @($profile.affected_paths).Count -or @($profile.sync_roots).Count) { throw 'Source profile lacks bounded workspace and native check contracts.' }
    $promptPath = (Resolve-Path -LiteralPath $PromptPath).Path
    if ([string]::IsNullOrWhiteSpace([IO.File]::ReadAllText($promptPath))) { throw 'A specific nonempty repair prompt is required.' }
    return @{ source_attempt = $SourceAttemptId; source_stage = $Stage; source_task = $stages[0].task; source_root = $scenarioRoot
        workspace = $Project; scenario = $Scenario; card = $cardPath; card_sha256 = Get-Sha256 $cardPath
        profile = $profilePath; profile_sha256 = Get-Sha256 $profilePath; prompt = $promptPath; prompt_sha256 = Get-Sha256 $promptPath }
}

function Assert-CampaignRepairInputs($Binding) {
    foreach ($field in 'card', 'profile', 'prompt') {
        if ((Get-Sha256 $Binding[$field]) -ne $Binding["${field}_sha256"]) { throw "Repair source $field changed; refusing to proceed." }
    }
}

function Get-CampaignRepairProtectedFiles($Binding) {
    Assert-CampaignRepairInputs $Binding
    $required = if ($Binding.scenario -eq 'A') { 'tests/health.test.ts' } else { 'src/Inventory.Web/appsettings.Development.json' }
    $regression = if ($Binding.scenario -eq 'A') { 'tests/regressions.test.ts' } else { 'tests/Inventory.Tests/RegressionTests.cs' }
    $allowed = @($required, $regression)
    $expected = @{}; $origins = @{}; $evidence = @{}
    $baselinePath = Join-Path $Binding.source_root 'results/protected-files.json'
    if (Test-Path -LiteralPath $baselinePath) {
        $baseline = Get-Content -LiteralPath $baselinePath -Raw | ConvertFrom-Json -AsHashtable
        if ($baseline.schema -ne 'vcp-protected-files/1' -or $baseline.workspace -ine $Binding.workspace -or
            -not $baseline.files.Contains($required) -or @($baseline.files.Keys | Where-Object { $_ -notin $allowed }).Count) { throw 'Invalid source Full protected-file baseline.' }
        foreach ($relative in $baseline.files.Keys) { $expected[$relative] = $baseline.files[$relative]; $origins[$relative] = 'source-full-baseline' }
        $evidence[$baselinePath] = Get-Sha256 $baselinePath
    }
    else {
        # Older Full runs did not retain their baseline map. A completed source
        # checkpoint still supplies immutable protected bytes; validate its copy.
        $checkpoints = Join-Path $Binding.source_root 'checkpoints'
        if (Test-Path -LiteralPath $checkpoints) {
            foreach ($manifestFile in Get-ChildItem -LiteralPath $checkpoints -Filter manifest.json -File -Recurse) {
                $manifest = Get-Content -LiteralPath $manifestFile.FullName -Raw | ConvertFrom-Json -AsHashtable
                if ($manifest.schema -ne 'vcp-source-checkpoint/1' -or $manifest.workspace -ine $Binding.workspace) { throw 'Invalid source Full checkpoint identity.' }
                foreach ($relative in $allowed) {
                    if (-not $manifest.files.Contains($relative)) { continue }
                    $hash = [string]$manifest.files[$relative]
                    $copy = Join-Path (Join-Path $manifestFile.DirectoryName 'files') $relative
                    if ($hash -notmatch '^[0-9a-f]{64}$' -or (Get-Sha256 $copy) -ne $hash -or
                        ($expected.ContainsKey($relative) -and $expected[$relative] -ne $hash)) { throw 'Source protected checkpoint is corrupt or has conflicting hashes.' }
                    $expected[$relative] = $hash; $origins[$relative] = 'source-full-checkpoint'
                    $evidence[$manifestFile.FullName] = Get-Sha256 $manifestFile.FullName
                    $evidence[$copy] = $hash
                }
            }
        }
    }
    foreach ($relative in $allowed) {
        $path = Join-Path $Binding.workspace $relative
        if (-not $expected.ContainsKey($relative) -and ($relative -eq $required -or (Test-Path -LiteralPath $path))) {
            $expected[$relative] = Get-Sha256 $path
            $origins[$relative] = 'current-before-only: source Full did not retain this protected hash'
        }
    }
    if ($Binding.source_stage -match '^T[4-6]' -and -not $expected.ContainsKey($regression)) { throw 'Source stage requires its protected regression file.' }
    foreach ($hash in $expected.Values) { if ($hash -notmatch '^[0-9a-f]{64}$') { throw 'Invalid protected-file hash.' } }
    return @{ schema = 'vcp-repair-protected-files/1'; workspace = $Binding.workspace; files = $expected; origins = $origins; evidence = $evidence }
}

function Assert-CampaignRepairProtectedFiles($Proof) {
    foreach ($path in $Proof.evidence.Keys) {
        if ((Get-Sha256 $path) -ne $Proof.evidence[$path]) { throw 'Source protected-file evidence changed.' }
    }
    foreach ($relative in $Proof.files.Keys) {
        $path = Join-Path $Proof.workspace $relative
        $probe = [IO.Path]::GetFullPath($path)
        while ($probe) {
            if ((Test-Path -LiteralPath $probe) -and ((Get-Item -LiteralPath $probe -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Protected files cannot traverse links.' }
            $probe = Split-Path -Parent $probe
        }
        if ((Get-Sha256 $path) -ne $Proof.files[$relative]) { throw "Protected file differs from its retained baseline: $relative" }
    }
}

function New-CampaignRepairProfile($Ctx, $Binding) {
    Assert-CampaignRepairInputs $Binding
    $profile = Get-Content -LiteralPath $Binding.profile -Raw | ConvertFrom-Json -AsHashtable -Depth 100
    if ($profile.workspace -ine $Ctx.Workspace) { throw 'Repair profile workspace mismatch.' }
    # Preserve every original process/check/tool/path/effect contract. Only the
    # run-owned scratch environment moves; old evidence must not become writable scratch.
    foreach ($process in $profile.processes) {
        foreach ($key in @($process.environment.Keys)) {
            $value = [string]$process.environment[$key]
            foreach ($directory in 'tmp', 'env') {
                $old = Join-Path $Binding.source_root $directory
                if ($value -ieq $old -or $value.StartsWith($old + '\', [StringComparison]::OrdinalIgnoreCase)) {
                    $replacement = [IO.Path]::GetFullPath($(if ($directory -eq 'tmp') { $Ctx.Temp } else { $Ctx.Env }))
                    $process.environment[$key] = $replacement + $value.Substring($old.Length)
                    if ($key -in 'TEMP', 'TMP', 'APPDATA', 'LOCALAPPDATA', 'DOTNET_CLI_HOME') {
                        $scratch = [IO.Path]::GetFullPath($process.environment[$key])
                        if ($scratch -ine $replacement -and -not $scratch.StartsWith($replacement + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Repair scratch environment escapes its new run.' }
                        [void][IO.Directory]::CreateDirectory($scratch)
                    }
                }
            }
        }
    }
    $profile.provider = $Ctx.SnapshotText | ConvertFrom-Json -AsHashtable -Depth 100
    $profile.catalog = $Ctx.Catalog
    $profile.budget_usd = Format-Usd $Ctx.TurnBudgetUsd
    $profile.deadline_seconds = $Ctx.DeadlineSeconds
    $profile.max_requests = $Ctx.MaxRequests
    $profile.output_tokens = [string]$Ctx.OutputTokens
    $profile.provider_timeout_seconds = [Math]::Min(300, $Ctx.DeadlineSeconds)
    $path = Join-Path $Ctx.Profiles 'targeted-repair.json'
    Write-JsonFile $path $profile
    return $path
}

function Assert-CampaignRepairVerification($Bundle, $Run, $Profile) {
    $expectedChecks = @($Profile.checks | ForEach-Object { [string]$_.manifest + '#test' })
    if (-not $expectedChecks.Count -or @($expectedChecks | Sort-Object -Unique).Count -ne $expectedChecks.Count) { throw 'Original verification check specifications must be nonempty and unique.' }
    if ($Run.exit_code -ne 0 -or $Bundle.kind -ne 'inspection_bundle' -or $Bundle.task.state -ne 'completed' -or
        $Bundle.task.scope.task -ne $Run.task) { throw 'Repair requires a completed native task with matching canonical scope.' }
    $pages = @($Bundle.views.verification)
    if (-not $pages.Count -or $pages[-1].next_cursor) { throw 'Incomplete native verification pages.' }
    $records = @()
    foreach ($page in $pages) {
        if ($page.scope.task -ne $Run.task -or $page.source_watermark -ne $Bundle.source_watermark -or @($page.gaps).Count) { throw 'Native verification scope or completeness mismatch.' }
        foreach ($item in $page.items) {
            if ($item.visibility -ne 'available' -or $item.collection -ne 'verification') { throw 'Unavailable native verification evidence.' }
            $record = $item.record
            $matches = $record.scope.task -eq $Run.task -and $record.scope.session -eq $Bundle.task.scope.session -and $record.scope.workspace -eq $Bundle.task.scope.workspace
            foreach ($field in 'repository', 'environment', 'buffers') { $matches = $matches -and $record.fingerprint[$field] -and $record.fingerprint[$field] -eq $Bundle.task.fingerprint[$field] }
            $observedChecks = @($record.checks | ForEach-Object { $_.specification })
            $checksMatch = $observedChecks.Count -eq $expectedChecks.Count -and @($observedChecks | Sort-Object -Unique).Count -eq $expectedChecks.Count -and
                @($expectedChecks | Where-Object { $_ -cnotin $observedChecks }).Count -eq 0
            if ($matches -and $checksMatch -and @($record.outstanding_issues).Count -eq 0 -and @($record.unresolved_effects).Count -eq 0 -and
                @($record.checks | Where-Object { $_.exit_code -ne 0 -or $_.outcome.status -ne 'passed' }).Count -eq 0) { $records += $record }
        }
    }
    if (-not $records.Count) { throw 'No successful native verification matches the completed task fingerprint and original checks.' }
}

Export-ModuleMember -Function Get-CampaignRepairSource, Assert-CampaignRepairInputs, Get-CampaignRepairProtectedFiles, Assert-CampaignRepairProtectedFiles, New-CampaignRepairProfile, Assert-CampaignRepairVerification
