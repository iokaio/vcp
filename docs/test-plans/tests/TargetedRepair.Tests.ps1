#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -DisableNameChecking
Import-Module (Join-Path $PSScriptRoot '../VcpAbCampaign.psm1') -Force
Import-Module (Join-Path $PSScriptRoot '../VcpCampaignRepair.psm1') -Force
$checks = 0
function Check($Value, $Message) { if (-not $Value) { throw $Message }; $script:checks++ }
function Reject([scriptblock]$Action, $Message) { $threw = $false; try { & $Action } catch { $threw = $true }; Check $threw $Message }
function Copy-Value($Value) { $Value | ConvertTo-Json -Depth 100 | ConvertFrom-Json -AsHashtable -Depth 100 }
$root = Join-Path ([IO.Path]::GetTempPath()) ('repair-tests-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
try {
    $workspace = Join-Path $root 'app'; $original = Join-Path $root 'original'; $scenarioRoot = Join-Path $original 'scenario/run'
    foreach ($path in $workspace, "$scenarioRoot/results", "$scenarioRoot/profiles", "$root/new/profiles") { [void][IO.Directory]::CreateDirectory($path) }
    $profilePath = Join-Path $scenarioRoot 'profiles/T2.json'
    $profile = @{ version = 1; workspace = $workspace; trust_workspace = $true; maximum_autonomy = 'autonomous'; sync_roots = @()
        affected_paths = @('src', 'tests'); checks = @(@{ kind = 'dotnet'; expected_tests = @('Must_run'); manifest = 'App.slnx'; runner = 'dotnet'; profile = 'dotnet' })
        processes = @(@{ name = 'dotnet'; executable = 'C:\dotnet.exe'; required_isolation = @(); reduced_isolation = $true; inputs = @(); max_timeout_ms = 300000
            environment = @{ TEMP = "$scenarioRoot\tmp"; DOTNET_CLI_HOME = "$scenarioRoot\env\dotnet"; PATH = 'public-path' } })
        canonical_tools = @('vcp_read','vcp_patch','vcp_exec','vcp_verify'); automatic_effects = @('read','write','execute','publish'); provider = @{ old = $true }; catalog = 'old'
        budget_usd = '30'; deadline_seconds = 1800; max_requests = 96; output_tokens = '8192'; provider_timeout_seconds = 300; max_transport_retries = 2 }
    Write-JsonFile $profilePath $profile
    $cardPath = Join-Path $scenarioRoot 'results/scorecard.json'
    $sourceCard = @{ schema = 'vcp-practical-scenario/1'; scenario = 'b-aspnet-inventory'; workspace = $workspace; dry_run = $false; verdict = 'fail'
        paid_execution_block = @{ reasons = @('budget_exhausted') }; stages = @(@{ stage = 'T2-api'; task = 'old-task'; skipped = $null; profile = $profilePath }) }
    Write-JsonFile $cardPath $sourceCard
    $prompt = Join-Path $root 'repair.md'; [IO.File]::WriteAllText($prompt, 'Repair only the per-factory database name; keep all assertions.')
    $source = @{ id = 'original'; mode = 'Full'; status = 'accounted'; scenario = 'B'; project = $workspace; timed_out = $false; exit_code = 1
        scorecard = $cardPath; root = $original; liability_usd = 9; cap_usd = 52; verdict = 'fail' }
    $state = @{ schema = 'vcp-ab-campaign/1'; authorized_usd = 100; attempts = @($source) }
    $binding = Get-CampaignRepairSource $state 'original' 'T2-api' 'B' $workspace $prompt
    Check ($binding.source_task -eq 'old-task') 'Lost original task binding.'
    Assert-CampaignRepairInputs $binding; $checks++
    foreach ($status in 'running','unresolved','billing-quarantined') {
        $bad = Copy-Value $state; $bad.attempts[0].status = $status
        Reject { Get-CampaignRepairSource $bad 'original' 'T2-api' 'B' $workspace $prompt } "Accepted $status source."
    }
    Reject { Get-CampaignRepairSource $state 'original' 'T1' 'B' $workspace $prompt } 'Accepted unrelated stage.'
    Reject { Get-CampaignRepairSource $state 'original' 'T2-api' 'A' $workspace $prompt } 'Accepted unrelated scenario.'
    Reject { Get-CampaignRepairSource $state 'original' 'T2-api' 'B' "$workspace-other" $prompt } 'Accepted unrelated workspace.'
    $ctx = @{ Workspace = $workspace; Profiles = "$root/new/profiles"; Temp = "$root/new/tmp"; Env = "$root/new/env"; SnapshotText = '{"selected":"fresh"}'
        Catalog = 'fresh-catalog'; TurnBudgetUsd = 5; DeadlineSeconds = 900; MaxRequests = 48; OutputTokens = 4096 }
    $newPath = New-CampaignRepairProfile $ctx $binding
    $new = Get-Content $newPath -Raw | ConvertFrom-Json -AsHashtable -Depth 100
    foreach ($field in 'checks','affected_paths','canonical_tools','automatic_effects') {
        Check (($new[$field] | ConvertTo-Json -Depth 20 -Compress) -ceq ($profile[$field] | ConvertTo-Json -Depth 20 -Compress)) "Changed original $field."
    }
    Check ($new.provider.selected -eq 'fresh' -and $new.budget_usd -eq '5.00') 'Did not use selected provider and task cap.'
    Check ($new.processes[0].environment.TEMP -eq [IO.Path]::GetFullPath($ctx.Temp) -and $new.processes[0].environment.DOTNET_CLI_HOME -eq [IO.Path]::GetFullPath($ctx.Env + '\dotnet')) 'Old evidence scratch paths remained writable.'
    Check ($new.processes[0].environment.PATH -eq 'public-path' -and $new.processes[0].executable -eq $profile.processes[0].executable) 'Process contract widened.'
    Assert-CampaignRepairInputs $binding; $checks++
    [IO.File]::AppendAllText($prompt, ' changed')
    Reject { Assert-CampaignRepairInputs $binding } 'Changed prompt accepted.'
    [IO.File]::WriteAllText($prompt, 'Repair only the per-factory database name; keep all assertions.')
    $bad = Copy-Value $profile; $bad.workspace = 'C:\other'; Write-JsonFile $profilePath $bad
    Reject { Get-CampaignRepairSource $state 'original' 'T2-api' 'B' $workspace $prompt } 'Unbound profile accepted.'
    Write-JsonFile $profilePath $profile
    $attempt = @{ id = 'repair'; scenario = 'B'; mode = 'Repair'; project = $workspace; executable = 'C:\vcp.exe'; status = 'running'; cap_usd = 5; liability_usd = 5
        fingerprint = 'fresh'; output_fingerprint = $null; repair_note = 'Fix fixture database lifetime'; verdict = 'incomplete' }
    Add-CampaignAttempt $state $attempt
    Check ((Get-CampaignLiability $state) -eq 14) 'Repair omitted full reservation or previous spend.'
    $repairCard = @{ schema = 'vcp-practical-scenario/1'; scenario = 'b-targeted-repair'; workspace = $workspace; vcp = 'C:\vcp.exe'; dry_run = $false; max_scenario_usd = 5
        verdict = 'pass'; spend_evidence_complete = $false; spend_usd = 0.2; paid_execution_block = $null; gates = @() }
    Complete-CampaignAttempt $attempt $repairCard 0 $false
    Check ($attempt.status -eq 'unresolved' -and $attempt.liability_usd -eq 5) 'Unknown repair accounting released its cap.'
    Reject { Add-CampaignAttempt $state (Copy-Value $attempt) } 'Unknown repair permitted another paid attempt.'
    $repairCard.spend_evidence_complete = $true
    Complete-CampaignAttempt $attempt $repairCard 0 $true
    Check ($attempt.liability_usd -eq 5 -and $attempt.verdict -eq 'repair-fail') 'Timed-out repair passed or released liability.'
    Complete-CampaignAttempt $attempt $repairCard 0 $false
    Check ($attempt.status -eq 'accounted' -and $attempt.liability_usd -eq 0.2 -and $attempt.verdict -eq 'repair-pass') 'Repair did not settle or was confused with Full pass.'
    $full = Copy-Value $attempt; $full.mode = 'Full'; $full.liability_usd = 5
    Reject { Complete-CampaignAttempt $full $repairCard 0 $false } 'Repair scorecard satisfied Full.'
    $huge = Copy-Value $attempt; $huge.liability_usd = 95
    Reject { Add-CampaignAttempt $state $huge } 'Repair exceeded global USD100.'
    $fingerprint = @{ repository = 'r'; environment = 'e'; buffers = 'b' }; $scope = @{ task = 'new-task'; session = 's'; workspace = 'w' }
    $verification = @{ scope = $scope; fingerprint = $fingerprint; outstanding_issues = @(); unresolved_effects = @(); checks = @(@{ specification = 'App.slnx#test'; exit_code = 0; outcome = @{ status = 'passed' } }) }
    $bundle = @{ kind = 'inspection_bundle'; source_watermark = '1'; task = @{ state = 'completed'; scope = $scope; fingerprint = $fingerprint }
        views = @{ verification = @(@{ scope = $scope; source_watermark = '1'; gaps = @(); next_cursor = $null
                items = @(@{ collection = 'verification'; visibility = 'available'; record = $verification }) }) } }
    $run = @{ task = 'new-task'; exit_code = 0 }
    Assert-CampaignRepairVerification $bundle $run $profile; $checks++
    foreach ($mutation in @(
        { param($b) $b.task.state = 'paused' },
        { param($b) $b.views.verification[0].items[0].record.fingerprint.repository = 'stale' },
        { param($b) $b.views.verification[0].items[0].record.checks[0].outcome.status = 'failed' },
        { param($b) $b.views.verification[0].items[0].record.checks = @() },
        { param($b) $b.views.verification[0].items[0].record.checks[0].specification = 'Other.slnx#test' },
        { param($b) $b.views.verification[0].items[0].record.checks += $b.views.verification[0].items[0].record.checks[0] },
        { param($b) $b.views.verification[0].items[0].record.outstanding_issues = @('failure') },
        { param($b) $b.views.verification[0].gaps = @('missing') },
        { param($b) $b.views.verification[0].next_cursor = 'more' },
        { param($b) $b.views.verification[0].scope.task = 'old-task' }
    )) { $bad = Copy-Value $bundle; & $mutation $bad; Reject { Assert-CampaignRepairVerification $bad $run $profile } 'Invalid/stale native verification was accepted.' }
    Check ((Get-Sha256 $cardPath) -eq $binding.card_sha256 -and $source.verdict -eq 'fail') 'Original failure evidence changed.'
    $tokens = $null; $errors = $null
    $tree = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../run-campaign-repair.ps1'), [ref]$tokens, [ref]$errors)
    Check (-not $errors.Count) 'Repair child failed parsing.'
    $calls = @($tree.FindAll({ param($n) $n -is [Management.Automation.Language.CommandAst] }, $true) | ForEach-Object { $_.GetCommandName() })
    Check (@($calls | Where-Object { $_ -eq 'Invoke-VcpTask' }).Count -eq 1 -and 'Invoke-RepairLoop' -notin $calls) 'Repair child can launch retries.'
    Check ('Save-Checkpoint' -in $calls -and 'Test-ProfileCheck' -in $calls -and 'Test-StageExit' -in $calls) 'Missing checkpoint/native admission/framing gates.'
    Write-Host "Targeted repair tests passed ($checks checks)."
}
finally { if ([IO.Path]::GetFullPath($root).StartsWith([IO.Path]::GetTempPath(), [StringComparison]::OrdinalIgnoreCase)) { Remove-Item -LiteralPath $root -Recurse -Force } }
