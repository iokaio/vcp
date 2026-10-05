#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '../VcpAbCampaign.psm1') -Force
Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -DisableNameChecking
$module = Import-Module (Join-Path $PSScriptRoot '../VcpCampaignRecovery.psm1') -Force -PassThru -DisableNameChecking
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-bound-quarantine-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temporary)
$checks = 0
function Check($condition, $message) { if (-not $condition) { throw $message }; $script:checks++ }
function Reject([scriptblock]$action, $message) { $failed = $false; try { & $action | Out-Null } catch { $failed = $true }; Check $failed $message }
function Clone($value) { return $value | ConvertTo-Json -Depth 100 | ConvertFrom-Json -AsHashtable -Depth 100 }
function Save-Audit($f) {
    [IO.File]::WriteAllText((Join-Path $f.root 'logs/vcp-commands.jsonl'), (($f.records | ForEach-Object { $_ | ConvertTo-Json -Depth 12 -Compress }) -join "`n"))
    [IO.File]::WriteAllText((Join-Path $f.root 'logs/vcp-commands.log'), (($f.records | ForEach-Object { "2026-10-04T00:00:01Z [T1/run] START id=$($_.id)" }) -join "`n"))
}
function New-Fixture {
    $root = Join-Path $temporary ([guid]::NewGuid().ToString('N')); $scenario = Join-Path $root 'run-id'
    [void][IO.Directory]::CreateDirectory((Join-Path $scenario 'logs/T1'))
    [void][IO.Directory]::CreateDirectory((Join-Path $scenario 'results'))
    $a = @{ id = 'attempt'; root = $root; status = 'billing-quarantined'; scenario = 'A'; mode = 'Full'; exit_code = 1; timed_out = $false; verdict = 'fail'
        executable = 'C:\vcp17.exe'; project = 'C:\isolated'; cap_usd = 30; liability_usd = 30; started = '2026-10-04T00:00:00Z'
        scorecard = Join-Path $scenario 'results/scorecard.json' }
    $card = @{ schema = 'vcp-practical-scenario/1'; scenario = 'a-vue-taskboard'; dry_run = $false; verdict = 'fail'; spend_evidence_complete = $false
        workspace = $a.project; vcp = $a.executable; max_scenario_usd = 30; run_id = 'run-id'; started = '2026-10-04T00:00:01Z'; finished = '2026-10-04T00:00:04Z'
        stages = @(@{ task = 'task'; skipped = $null; stage = 'T1' }) }
    $scope = @{ task = 'task'; session = 'session'; workspace = 'workspace' }
    $record = @{ id = '0123456789abcdef0123456789abcdef'; workspace = $a.project; executable = $a.executable; completed_at = '2026-10-04T00:00:03Z'
        argv = @('--format', 'jsonl', '--non-interactive', '--workspace', $a.project, '--data-dir', (Join-Path $scenario 'vcp-data'), 'run', '--budget-usd', '5.00')
        accepted = $true; task = 'task'; session = 'session'; timed_out = $false; exit_code = 7; stdout_path = Join-Path $scenario 'logs/run.stdout.jsonl' }
    $frames = @(@{ schema_version = 1; type = 'accepted'; correlation = 'command'; scope = $scope }, @{ schema_version = 1; type = 'result'; correlation = 'command'; scope = $scope; exit_code = 7; conditions = @{ paused = $true } })
    [IO.File]::WriteAllText($record.stdout_path, (($frames | ForEach-Object { $_ | ConvertTo-Json -Depth 12 -Compress }) -join "`n"))
    [IO.File]::WriteAllText($a.scorecard, ($card | ConvertTo-Json -Depth 12))
    $bundle = @{ kind = 'inspection_bundle'; schema_version = 1; source_watermark = '100'
        task = @{ scope = $scope; root = 'task'; parent = $null; state = 'paused' }
        agents = @(@{ root = 'task'; total = 0; items = @(); next_offset = $null }); views = @{} }
    foreach ($name in 'costs', 'tools', 'verification', 'routing', 'policy', 'outputs') {
        $bundle.views[$name] = @(@{ view = $name; scope = $scope; source_watermark = '100'; gaps = @(); next_cursor = $null; items = @() })
    }
    $bundle.views.costs[0].items = @(
        @{ collection = 'ledger'; id = 'task'; visibility = 'available'; record = @{ scope = $scope; currency = 'USD'; cap = '5000000'; settled = '100000'; unresolved = '2000000'; active = '0'; allocations = @{}; overrun = $false } },
        @{ collection = 'attempt'; id = 'provider'; visibility = 'available'; record = @{ id = 'provider'; scope = $scope; root = 'task'; reservation = 'reservation'; phase = 'reconciliation_pending'; charged = '100000'; quote = @{ amount = @{ currency = 'USD'; micros = '2500000' } } } },
        @{ collection = 'reservation'; id = 'reservation'; visibility = 'available'; record = @{ id = 'reservation'; scope = $scope; root = 'task'; attempt = 'provider'; phase = 'reconciliation_pending'; charged = '100000'; liability = '2000000'; amount = @{ currency = 'USD'; micros = '2500000' } } }
    )
    $bundle.views.tools[0].items = @(@{ collection = 'effect'; id = 'effect'; visibility = 'available'; record = @{ scope = $scope; state = 'succeeded' } })
    $bundlePath = Join-Path $scenario 'logs/T1/inspection-bundle.json'
    [IO.File]::WriteAllText($bundlePath, ($bundle | ConvertTo-Json -Depth 100))
    $proofPath = Join-Path $root 'billing-quarantine.json'
    $proof = @{ schema = 'vcp-ab-billing-quarantine/1'; attempt = $a.id; permanent_hold_usd = 30; resume_forbidden = $true; workspace_reuse_forbidden = $true
        native_processes_referencing_data = 0; tasks = @('task'); evidence_files = @($a.scorecard, $bundlePath | ForEach-Object { @{ path = $_; sha256 = (Get-FileHash $_).Hash.ToLowerInvariant() } }) }
    [IO.File]::WriteAllText($proofPath, ($proof | ConvertTo-Json -Depth 100))
    $a.quarantine = @{ evidence = $proofPath; sha256 = (Get-FileHash $proofPath).Hash.ToLowerInvariant(); permanent_hold_usd = 30 }
    $f = @{ attempt = $a; card = $card; root = $scenario; records = @($record); bundle = $bundle; bundlePath = $bundlePath }
    Save-Audit $f
    return $f
}
try {
    # A supported automatic metadata refresh is not an unclassified paid probe.
    $metadataFixture = New-Fixture
    $metadataRecord = Clone $metadataFixture.records[0]
    $metadataRecord.id = '2123456789abcdef0123456789abcdef'
    $metadataRecord.argv = @('setup','provider-metadata','--model','fixture/model','--endpoint','fixture','--output',(Join-Path $metadataFixture.root 'provider-current'))
    $metadataRecord.accepted = $false; $metadataRecord.task = $null; $metadataRecord.session = $null; $metadataRecord.exit_code = 0
    $metadataRecord.stdout_path = Join-Path $metadataFixture.root 'logs/metadata.stdout.jsonl'
    $metadataFrame = @{schema_version=1;type='result';exit_code=0;scope=$null;correlation='metadata';data=@{model_calls=0;model='fixture/model';endpoint='fixture';status='created'}}
    [IO.File]::WriteAllText($metadataRecord.stdout_path,($metadataFrame|ConvertTo-Json -Depth 12 -Compress))
    $metadataFixture.records += $metadataRecord; Save-Audit $metadataFixture
    Check ((Get-CampaignQuarantineInputs $metadataFixture.attempt $metadataFixture.card $metadataFixture.root).inventory.tasks.Count -eq 1) 'Metadata-only capture changed the dispatched root count.'
    $metadataFrame.data.model_calls=1
    [IO.File]::WriteAllText($metadataRecord.stdout_path,($metadataFrame|ConvertTo-Json -Depth 12 -Compress))
    Reject {Get-CampaignQuarantineInputs $metadataFixture.attempt $metadataFixture.card $metadataFixture.root} 'A metadata result reporting model calls was treated as nonbillable.'
    $metadataFrame.data.model_calls=0
    [IO.File]::WriteAllText($metadataRecord.stdout_path,($metadataFrame|ConvertTo-Json -Depth 12 -Compress))
    $metadataRecord.argv += '--probe'; Save-Audit $metadataFixture
    Reject {Get-CampaignQuarantineInputs $metadataFixture.attempt $metadataFixture.card $metadataFixture.root} 'Additional metadata probe arguments were accepted.'
    $f = New-Fixture
    $inputs = Get-CampaignQuarantineInputs $f.attempt $f.card $f.root
    $before = $f.attempt | ConvertTo-Json -Depth 100 -Compress
    $bound = Assert-CampaignQuarantineBound $f.attempt $inputs.inventory $inputs.original @($f.bundle)
    Check ($bound.bounded_liability_usd -eq 5 -and -not $bound.accounting_complete -and $bound.resume_forbidden -and $bound.workspace_reuse_forbidden) 'Root cap, unknown billing or isolation was lost.'
    Check (($f.attempt | ConvertTo-Json -Depth 100 -Compress) -ceq $before) 'Validation changed the original attempt.'
    $second = Clone $f.bundle
    $second.task.root = 'second'; $second.agents[0].root = 'second'; $second.task.scope.task = 'second'
    foreach ($name in $second.views.Keys) {
        foreach ($page in $second.views[$name]) {
            $page.scope.task = 'second'
            foreach ($item in $page.items) { $item.record.scope.task = 'second'; if ($item.record.ContainsKey('root')) { $item.record.root = 'second' } }
        }
    }
    $second.views.costs[0].items[0].id = 'second'
    $multi = @{ tasks = @($inputs.inventory.tasks[0], @{ task = 'second'; session = 'session'; workspace = 'workspace'; original_cap_usd = 5 }) }
    Check ((Assert-CampaignQuarantineBound $f.attempt $multi @($f.bundle, $second) @($f.bundle, $second)).bounded_liability_usd -eq 10) 'Two dispatched roots did not retain both complete original caps.'
    foreach ($mutation in @(
        { param($f) $f.attempt.timed_out = $true },
        { param($f) $f.attempt.status = 'running' },
        { param($f) $f.attempt.bounded_liability = @{} },
        { param($f) $f.attempt.liability_usd = 5 },
        { param($f) $f.card.workspace = 'C:\other' },
        { param($f) $f.records[0].argv[-1] = '0'; Save-Audit $f },
        { param($f) $f.records[0].argv = @('run'); Save-Audit $f },
        { param($f) $f.records[0].argv += @('--budget-usd','5'); Save-Audit $f },
        { param($f) $f.records[0].argv[-3] = 'resume'; Save-Audit $f },
        { param($f) $other = Clone $f.records[0]; $other.id = '1123456789abcdef0123456789abcdef'; $f.records += $other; Save-Audit $f },
        { param($f) [IO.File]::AppendAllText((Join-Path $f.root 'logs/vcp-commands.log'), "`n2026-10-04T00:00:01Z [T2/run] START id=1123456789abcdef0123456789abcdef") },
        { param($f) [IO.File]::AppendAllText($f.records[0].stdout_path, "`ntruncated") },
        { param($f) [IO.File]::AppendAllText($f.attempt.scorecard, ' ') },
        { param($f) [IO.File]::AppendAllText($f.bundlePath, ' ') },
        { param($f) [IO.File]::AppendAllText($f.attempt.quarantine.evidence, ' ') },
        { param($f) $f.records[0].accepted = $false; Save-Audit $f },
        { param($f) $f.records[0].argv = @('setup','provider','probe'); $f.records[0].accepted = $false; Save-Audit $f },
        { param($f) [IO.File]::WriteAllText((Join-Path $f.attempt.root 'vcp-commands.log'), 'orphan') }
    )) {
        $bad = New-Fixture; & $mutation $bad
        Reject { Get-CampaignQuarantineInputs $bad.attempt $bad.card $bad.root } 'Invalid original inventory or changed quarantine evidence was accepted.'
    }
    foreach ($mutation in @(
        { param($b) $b.task.state = 'running' }, { param($b) $b.task.parent = 'parent' }, { param($b) $b.task.root = 'other' },
        { param($b) $b.task.scope.workspace = 'other' }, { param($b) $b.source_watermark = '99' },
        { param($b) $b.agents[0].total = 1 }, { param($b) $b.agents[0].next_offset = 2 },
        { param($b) $b.views.costs[0].next_cursor = 'more' }, { param($b) $b.views.costs[0].gaps = @('missing') },
        { param($b) $b.views.costs[0].items += $b.views.costs[0].items[0] },
        { param($b) $b.views.costs[0].items[0].visibility = 'hidden' },
        { param($b) $b.views.costs[0].items[0].record.active = '1' },
        { param($b) $b.views.costs[0].items[0].record.overrun = $true },
        { param($b) $b.views.costs[0].items[0].record.cap = '6000000' },
        { param($b) $b.views.costs[0].items[0].record.allocations.child = '1' },
        { param($b) $b.views.costs[0].items[0].record.settled = '9999999' },
        { param($b) $b.views.costs[0].items[1].record.phase = 'submitted' },
        { param($b) $b.views.costs[0].items[2].record.attempt = 'other' },
        { param($b) $b.views.costs[0].items[2].record.liability = '0'; $b.views.costs[0].items[0].record.unresolved = '0' },
        { param($b) $b.views.costs[0].items[2].record.liability = '2000001' },
        { param($b) $b.views.tools[0].items[0].record.state = 'outcome_unknown' },
        { param($b) $b.views.tools[0].scope.session = 'other' },
        { param($b) $b.views.Remove('outputs') },
        { param($b) $b.views.routing[0].gaps = @(@{ reason = 'missing accounting'; visibility = 'unavailable' }) }
    )) {
        $bad = Clone $f.bundle; & $mutation $bad
        Reject { Assert-CampaignQuarantineBound $f.attempt $inputs.inventory $inputs.original @($bad) } 'Unsafe/incomplete fresh canonical evidence was accepted.'
    }
    # Exercise real inventory + evidence collector with actual JSONL object shape,
    # mocking only native reads. No provider credentials or inference are allowed.
    & $module {
        param($bundle)
        $script:boundBundle = $bundle; $script:readCalls = 0
        function script:Invoke-Vcp {
            param($Ctx, $Stage, $Label, $Arguments, $TimeoutSeconds, [switch]$NoGlobals, [switch]$DenyProviderCredentials)
            if (-not $DenyProviderCredentials -or $Arguments[0] -notin '--version', 'inspect-bundle') { throw 'Unexpected paid/credential-bearing operation.' }
            $script:readCalls++
            $path = Join-Path $Ctx.Logs "$Label.jsonl"
            if ($Arguments[0] -eq '--version') { [IO.File]::WriteAllText($path, 'vcp 0.2.21'); return @{ ExitCode = 0; TimedOut = $false; StdoutPath = $path } }
            $frame = @{ schema_version = 1; type = 'result'; exit_code = 0; correlation = 'metadata'; data = $script:boundBundle } | ConvertTo-Json -Depth 100 -Compress
            [IO.File]::WriteAllText($path, $frame)
            $parsed = $frame | ConvertFrom-Json -Depth 100
            return @{ ExitCode = 0; TimedOut = $false; InvalidLines = 0; Accepted = $null; Result = $parsed; Frames = @($parsed); StdoutPath = $path }
        }
    } $f.bundle
    $evidence = Invoke-CampaignQuarantineBound $f.attempt $f.card $f.root (Get-Process -Id $PID).Path 'One original root, retain its entire cap.'
    Check ($evidence.bounded_liability_usd -eq 5 -and $evidence.credential_denied -and $evidence.evidence_files.Count -ge 7) 'Credential-denied collection failed to retain its evidence.'
    Check ((& $module { $script:readCalls }) -eq 2 -and ($f.attempt | ConvertTo-Json -Depth 100 -Compress) -ceq $before) 'Collector mutated original state or made extra calls.'
    $f.attempt.liability_usd = 5; $f.attempt.bounded_liability = @{ retained_root_caps_usd = 5 }
    Reject { Get-CampaignQuarantineInputs $f.attempt $f.card $f.root } 'A second bound was accepted.'
    Reject { Complete-CampaignAttempt $f.attempt $f.card 1 $false } 'Normal reconciliation released a bounded quarantine.'
    Reject { Get-CampaignRecoveryInventory $f.attempt $f.card $f.root } 'Normal fresh-accounting recovery accepted a bounded quarantine.'
    $state = @{ schema = 'vcp-ab-campaign/1'; authorized_usd = 100; attempts = @($f.attempt) }
    foreach ($project in 'C:\isolated', 'C:\isolated\child', 'C:\') {
        $next = @{ status = 'running'; mode = 'DryRun'; scenario = 'B'; project = $project; liability_usd = 1 }
        Reject { Add-CampaignAttempt $state $next } 'Bounded quarantine workspace or ancestor/descendant was reused.'
    }
    Check ((Get-CampaignLiability $state) -eq 5 -and $f.attempt.cap_usd -eq 30 -and $f.attempt.quarantine.permanent_hold_usd -eq 30) 'Original cap audit or retained root liability changed.'
    # Execute the production maintenance branch, including real project/campaign
    # locks, create-new receipt and atomic ledger publication. Replace process
    # inventory only; native execution remains the credential-denied mock above.
    $tokens = $null; $parseErrors = $null
    $runner = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../run-ab-campaign.ps1'), [ref]$tokens, [ref]$parseErrors)
    Check ($parseErrors.Count -eq 0) 'Campaign controller has syntax errors.'
    $branch = $runner.Find({ param($node) $node -is [Management.Automation.Language.IfStatementAst] -and $node.Extent.Text.StartsWith('if ($Action -in') }, $false)
    $maintenance = [scriptblock]::Create($branch.Extent.Text.Replace('exit 0', 'return'))
    function Invoke-FixtureController($f, $maintenance, $action = 'BoundQuarantine') {
        $Action = $action; $CampaignRoot = $f.attempt.root; $AttemptId = $f.attempt.id
        $statePath = Join-Path $CampaignRoot 'campaign.json'; $RefreshAccounting = $false
        $Vcp = (Get-Process -Id $PID).Path; $RepairNote = 'Complete launched-root evidence retains all original caps.'
        function Get-CimInstance { return @() }
        & $maintenance
    }
    $f = New-Fixture
    $statePath = Join-Path $f.attempt.root 'campaign.json'
    Write-CampaignState $statePath @{ schema = 'vcp-ab-campaign/1'; authorized_usd = 100; attempts = @($f.attempt) }
    & $module { param($bundle) $script:boundBundle = $bundle } $f.bundle
    Invoke-FixtureController $f $maintenance
    $published = Get-Content $statePath -Raw | ConvertFrom-Json -AsHashtable
    $retained = $published.attempts[0]
    Check ($retained.status -eq 'billing-quarantined' -and $retained.liability_usd -eq 5 -and $retained.cap_usd -eq 30 -and $retained.quarantine.permanent_hold_usd -eq 30) 'Controller changed original cap/status or failed to retain launched-root caps.'
    Check ((Get-FileHash $retained.bounded_liability.evidence).Hash.ToLowerInvariant() -eq $retained.bounded_liability.sha256) 'New append-only proof is not hash-bound to the ledger.'
    Check ((Get-FileHash $f.attempt.quarantine.evidence).Hash.ToLowerInvariant() -eq $f.attempt.quarantine.sha256) 'Controller rewrote original quarantine proof.'
    Reject { Invoke-FixtureController $f $maintenance } 'Controller allowed a second bound.'
    Reject { Invoke-FixtureController $f $maintenance 'Reconcile' } 'Controller reconciled a bounded quarantine.'
    $f = New-Fixture
    $orphan = @{ id = 'orphan'; status = 'running'; project = 'C:\orphan' }
    Write-CampaignState (Join-Path $f.attempt.root 'campaign.json') @{ schema = 'vcp-ab-campaign/1'; authorized_usd = 100; attempts = @($f.attempt, $orphan) }
    Reject { Invoke-FixtureController $f $maintenance } 'Controller reduced a hold with orphaned running work.'
    Write-Host "Bounded quarantine checks passed: $checks"
}
finally {
    $full = [IO.Path]::GetFullPath($temporary)
    $parent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $full.StartsWith($parent, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $full -Leaf) -notlike 'vcp-bound-quarantine-*') { throw 'Unsafe test cleanup path.' }
    Remove-Item -LiteralPath $full -Recurse -Force
}
