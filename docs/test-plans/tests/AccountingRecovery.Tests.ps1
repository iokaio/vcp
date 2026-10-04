#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpCampaignRecovery.psm1') -Force -PassThru -DisableNameChecking
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-accounting-recovery-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$checks = 0
function Check($condition, $message) { if (-not $condition) { throw $message }; $script:checks++ }
function Reject([scriptblock]$action, $message) { $failed = $false; try { & $action | Out-Null } catch { $failed = $true }; Check $failed $message }
function New-Fixture {
    $root = Join-Path $temporary ([guid]::NewGuid().ToString('N'))
    $scenario = Join-Path $root 'run-id'
    [void][IO.Directory]::CreateDirectory((Join-Path $scenario 'logs'))
    $attempt = @{ id = 'attempt'; root = $root; status = 'unresolved'; scenario = 'A'; mode = 'Full'; exit_code = 1; timed_out = $false; verdict = 'fail'
        executable = 'C:\vcp17.exe'; project = 'C:\project'; cap_usd = 20; liability_usd = 20
        scorecard = Join-Path $scenario 'scorecard.json'; started = '2026-10-04T00:00:00Z' }
    $card = @{ schema = 'vcp-practical-scenario/1'; scenario = 'a-vue-taskboard'; dry_run = $false; verdict = 'fail'; spend_evidence_complete = $false; workspace = 'C:\project'; vcp = 'C:\vcp17.exe'; max_scenario_usd = 20
        run_id = 'run-id'; started = '2026-10-04T00:00:01Z'; finished = '2026-10-04T00:00:04Z'; stages = @(@{ task = 'task'; skipped = $null }) }
    $scope = @{ task = 'task'; session = 'session'; workspace = 'workspace' }
    $record = @{ id = '0123456789abcdef0123456789abcdef'; workspace = 'C:\project'; executable = 'C:\vcp17.exe'; completed_at = '2026-10-04T00:00:03Z'
        argv = @('--format', 'jsonl', '--non-interactive', '--workspace', 'C:\project', '--data-dir', (Join-Path $scenario 'vcp-data'), 'run', '--file', 'prompt.md')
        accepted = $true; task = 'task'; session = 'session'; timed_out = $false; exit_code = 0; stdout_path = Join-Path $scenario 'logs/run.stdout.jsonl' }
    $frames = @(@{ schema_version = 1; type = 'accepted'; correlation = 'command'; scope = $scope }, @{ schema_version = 1; type = 'result'; correlation = 'command'; scope = $scope; exit_code = 0; conditions = @{ completed = $true } })
    [IO.File]::WriteAllText($record.stdout_path, (($frames | ForEach-Object { $_ | ConvertTo-Json -Depth 12 -Compress }) -join "`n"))
    [IO.File]::WriteAllText($attempt.scorecard, ($card | ConvertTo-Json -Depth 12))
    $fixture = @{ attempt = $attempt; card = $card; root = $scenario; records = @($record); scope = $scope }
    Save-Audit $fixture
    return $fixture
}
function Save-Audit($fixture) {
    [IO.File]::WriteAllText((Join-Path $fixture.root 'logs/vcp-commands.jsonl'), (($fixture.records | ForEach-Object { $_ | ConvertTo-Json -Depth 12 -Compress }) -join "`n"))
    [IO.File]::WriteAllText((Join-Path $fixture.root 'logs/vcp-commands.log'), (($fixture.records | ForEach-Object { "2026-10-04T00:00:01Z [T1/run] START id=$($_.id)" }) -join "`n"))
}
function New-Bundle($scope) {
    return @{ kind = 'inspection_bundle'; schema_version = 1; source_watermark = '100'; task = @{ scope = $scope; state = 'completed' }
        agents = @(@{ root = $scope.task; total = 0; items = @(); next_offset = $null })
        views = @{
            costs = @(@{ view = 'costs'; scope = $scope; source_watermark = '100'; gaps = @(); next_cursor = $null; items = @(
                @{ collection = 'ledger'; visibility = 'available'; record = @{ scope = $scope; currency = 'USD'; cap = '5000000'; overrun = $false; settled = '500000'; active = '0'; unresolved = '0' } }
            ) })
            tools = @(@{ view = 'tools'; scope = $scope; source_watermark = '100'; gaps = @(); next_cursor = $null; items = @(
                @{ collection = 'effect'; visibility = 'available'; record = @{ scope = $scope; state = 'failed' } }
            ) })
        }
    }
}
try {
    $f = New-Fixture
    $originalHash = (Get-FileHash $f.attempt.scorecard).Hash
    $inventory = Get-CampaignRecoveryInventory $f.attempt $f.card $f.root
    $bundle = New-Bundle $f.scope
    Check ((Get-CampaignRecoveredCost $f.attempt $inventory @($bundle)) -eq [decimal]0.5) 'Canonical settled cost was not recovered.'
    $second = $f.records[0].Clone(); $second.id = '1123456789abcdef0123456789abcdef'
    $second.argv = @('--format', 'jsonl', '--non-interactive', '--workspace', 'C:\project', '--data-dir', (Join-Path $f.root 'vcp-data'), 'resume', 'task')
    $f.records += $second; Save-Audit $f
    $inventory = Get-CampaignRecoveryInventory $f.attempt $f.card $f.root
    Check ($inventory.tasks.Count -eq 1 -and (Get-CampaignRecoveredCost $f.attempt $inventory @($bundle)) -eq [decimal]0.5) 'Resume doubled cumulative task settlement.'
    $last = $second.Clone(); $last.id = '2123456789abcdef0123456789abcdef'
    $last.argv = @('--format', 'jsonl', '--non-interactive', '--workspace', 'C:\project', '--data-dir', (Join-Path $f.root 'vcp-data'), 'resume', '--last')
    $f.records += $last; Save-Audit $f
    $inventory = Get-CampaignRecoveryInventory $f.attempt $f.card $f.root
    Check ($inventory.tasks.Count -eq 1) 'The scenario A resume --last command was not deduplicated by its actual accepted task.'
    foreach ($mutation in @(
        { param($f) $f.attempt.status = 'billing-quarantined' },
        { param($f) $f.attempt.timed_out = $true },
        { param($f) $f.card.schema = 'unknown' },
        { param($f) $f.card.scenario = 'b-aspnet-inventory' },
        { param($f) $f.records[0].task = 'other'; Save-Audit $f },
        { param($f) $f.records[0].timed_out = $true; Save-Audit $f },
        { param($f) $f.records[0].argv[6] = 'wrong-data'; Save-Audit $f },
        { param($f) [IO.File]::AppendAllText((Join-Path $f.root 'logs/vcp-commands.log'), "`n2026-10-04T00:00:01Z [T2/run] START id=1123456789abcdef0123456789abcdef") },
        { param($f) [IO.File]::AppendAllText($f.records[0].stdout_path, "`ntruncated") },
        { param($f) [IO.File]::WriteAllText($f.records[0].stdout_path, '{"type":"result","exit_code":0,"scope":null,"receipt":null,"conditions":{"completed":true}}') }
    )) {
        $bad = New-Fixture; & $mutation $bad
        Reject { Get-CampaignRecoveryInventory $bad.attempt $bad.card $bad.root } 'Incomplete or ineligible original dispatch evidence was accepted.'
    }
    foreach ($mutation in @(
        { param($b) $b.task.state = 'running' },
        { param($b) $b.agents[0].total = 1 },
        { param($b) $b.views.tools[0].view = 'costs' },
        { param($b) $b.views.costs[0].gaps = @('missing') },
        { param($b) $b.views.tools[0].source_watermark = '99' },
        { param($b) $b.views.costs[0].next_cursor = 'more' },
        { param($b) $b.views.costs[0].items[0].record.active = '1' },
        { param($b) $b.views.costs[0].items[0].record.unresolved = '1' },
        { param($b) $b.views.tools[0].items[0].record.state = 'outcome_unknown' },
        { param($b) $b.views.tools[0].items[0].record.state = 'running' },
        { param($b) $b.views.costs[0].items[0].record.settled = '9000000' },
        { param($b) $b.views.costs[0].items[0].visibility = 'hidden' }
    )) {
        $bad = New-Bundle $f.scope; & $mutation $bad
        Reject { Get-CampaignRecoveredCost $f.attempt $inventory @($bad) } 'Incomplete/live/unknown canonical accounting was released.'
    }
    $f.attempt.status = 'billing-quarantined'
    Reject { Get-CampaignRecoveredCost $f.attempt $inventory @($bundle) } 'Permanent Azure hold could be released.'
    $f.attempt.status = 'unresolved'; $f.attempt.cap_usd = [decimal]0.4
    Reject { Get-CampaignRecoveredCost $f.attempt $inventory @($bundle) } 'Aggregate settlement exceeded the retained cap.'
    $f.attempt.cap_usd = 20
    # Mock only native reads. Exercise the real inventory/cost logic and prove
    # every executable operation strips credentials and is version/inspection.
    & $module {
        param($bundle)
        $script:recoveryBundle = $bundle; $script:readCalls = 0
        function script:Invoke-Vcp {
            param($Ctx, $Stage, $Label, $Arguments, $TimeoutSeconds, [switch]$NoGlobals, [switch]$DenyProviderCredentials)
            if (-not $DenyProviderCredentials -or $Arguments[0] -notin '--version', 'inspect-bundle') { throw 'Recovery attempted a paid or credential-bearing operation.' }
            $script:readCalls++
            $path = Join-Path $Ctx.Logs "$Label.jsonl"
            [IO.File]::WriteAllText($path, $(if ($Arguments[0] -eq '--version') { 'vcp 0.2.19' } else { $script:recoveryBundle | ConvertTo-Json -Depth 32 }))
            return @{ ExitCode = 0; TimedOut = $false; InvalidLines = 0; Accepted = $null; Result = @{ data = $script:recoveryBundle }; Frames = @(@{ type = 'result' }); StdoutPath = $path }
        }
    } $bundle
    $recovery = Invoke-CampaignAccountingRecovery $f.attempt $f.card $f.root (Get-Process -Id $PID).Path
    Check ($recovery.settled_usd -eq [decimal]0.5 -and $recovery.executable_version -eq 'vcp 0.2.19') 'Metadata-only recovery did not produce supplemental evidence.'
    Check ((& $module { $script:readCalls }) -eq 2) 'A resumed task caused duplicate inspections or unexpected calls.'
    Check ((Get-FileHash $f.attempt.scorecard).Hash -eq $originalHash -and $f.attempt.verdict -eq 'fail' -and $f.attempt.liability_usd -eq 20) 'Recovery collection rewrote original failure evidence or allocation.'
    Write-Host "Accounting recovery checks passed: $checks"
}
finally {
    $full = [IO.Path]::GetFullPath($temporary)
    $parent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $full.StartsWith($parent, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $full -Leaf) -notlike 'vcp-accounting-recovery-*') { throw 'Unsafe recovery test cleanup path.' }
    Remove-Item -LiteralPath $full -Recurse -Force
}
