#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '../VcpAbCampaign.psm1') -Force
$checks = 0
function Check($Condition, $Message) { if (-not $Condition) { throw $Message }; $script:checks++ }
function Reject([scriptblock]$Action, $Message) { $failed = $false; try { & $Action | Out-Null } catch { $failed = $true }; Check $failed $Message }
function Fixture {
    $attempt = @{ id = 'old'; status = 'unresolved'; mode = 'Full'; scenario = 'B'; exit_code = 1; timed_out = $false; liability_usd = 30; cap_usd = 30
        project = 'C:\old'; executable = 'C:\vcp.exe'; verdict = 'fail'; fingerprint = 'old'; output_fingerprint = 'old-after' }
    $card = @{ schema = 'vcp-practical-scenario/1'; scenario = 'b-aspnet-inventory'; max_scenario_usd = 30; dry_run = $false
        workspace = 'C:\old'; vcp = 'C:\vcp.exe'; verdict = 'fail'; spend_evidence_complete = $false; paid_execution_block = $null; gates = @()
        stages = @(@{ stage = 'T1'; task = 'task'; skipped = $null; exit_code = 7 }) }
    $bundle = @{ kind = 'inspection_bundle'; schema_version = 1; source_watermark = '10'
        task = @{ scope = @{ task = 'task' }; state = 'paused'; editing = $false }
        agents = @{ root = 'task'; total = 0; items = @(); next_offset = $null }
        views = @{
            costs = @{ view = 'costs'; scope = @{ task = 'task' }; source_watermark = '10'; gaps = @(); next_cursor = $null; items = @(
                @{ collection = 'ledger'; visibility = 'available'; record = @{ scope = @{ task = 'task' }; currency = 'USD'; active = '0'; unresolved = '2000000'; settled = '100000'; cap = '5000000'; overrun = $false } },
                @{ collection = 'reservation'; visibility = 'available'; record = @{ scope = @{ task = 'task' }; phase = 'reconciliation_pending'; attempt = 'provider-attempt'; liability = '2000000' } },
                @{ collection = 'attempt'; id = 'provider-attempt'; visibility = 'available'; record = @{ scope = @{ task = 'task' }; phase = 'reconciliation_pending' } }
            ) }
            tools = @{ view = 'tools'; scope = @{ task = 'task' }; source_watermark = '10'; gaps = @(); next_cursor = $null; items = @(
                @{ collection = 'effect'; visibility = 'available'; record = @{ scope = @{ task = 'task' }; state = 'succeeded' } }
            ) }
        }
    }
    return @{ attempt = $attempt; card = $card; bundle = $bundle }
}
$f = Fixture
$evidence = Assert-CampaignBillingQuarantine $f.attempt $f.card @($f.bundle) 'Unknown provider bill permanently retained; fresh isolated retry.'
Check ($evidence.permanent_hold_usd -eq 30 -and $evidence.unresolved_provider_usd -eq 2 -and $evidence.resume_forbidden) 'Quarantine did not retain full allocation or prohibit replay.'
Check ($f.attempt.status -eq 'unresolved' -and $f.attempt.liability_usd -eq 30) 'Evidence validation mutated original accounting.'
foreach ($mutation in @(
    { param($f) $f.attempt.timed_out = $true },
    { param($f) $f.attempt.exit_code = $null },
    { param($f) $f.attempt.liability_usd = 5 },
    { param($f) $f.card.workspace = 'C:\other' },
    { param($f) $f.bundle.task.scope.task = 'other' },
    { param($f) $f.bundle.task.state = 'running' },
    { param($f) $f.bundle.agents.total = 1 },
    { param($f) $f.bundle.agents.next_offset = 'next' },
    { param($f) $f.bundle.views.costs.gaps = @('missing') },
    { param($f) $f.bundle.views.tools.next_cursor = 'next' },
    { param($f) $f.bundle.views.tools.source_watermark = '9' },
    { param($f) $f.bundle.views.tools.view = 'costs' },
    { param($f) $f.bundle.views.tools.items[0].record.state = 'unknown' },
    { param($f) $f.bundle.views.tools.items[0].visibility = 'hidden' },
    { param($f) $f.bundle.views.costs.items[0].record.active = '1' },
    { param($f) $f.bundle.views.costs.items[0].record.overrun = $true },
    { param($f) $f.bundle.views.costs.items[0].record.unresolved = '2000001' },
    { param($f) $f.bundle.views.costs.items[2].record.phase = 'submitted' },
    { param($f) $f.bundle.views.costs.items[1].record.attempt = 'missing' }
)) {
    $f = Fixture
    & $mutation $f
    Reject { Assert-CampaignBillingQuarantine $f.attempt $f.card @($f.bundle) 'diagnosed' } 'Unsafe or incomplete billing quarantine was accepted.'
}
$f = Fixture
$f.bundle.task.editing = $true
$f.bundle.agents = @($f.bundle.agents)
$f.bundle.views.costs = @($f.bundle.views.costs)
$f.bundle.views.tools = @($f.bundle.views.tools)
Check ((Assert-CampaignBillingQuarantine $f.attempt $f.card @($f.bundle) 'diagnosed').permanent_hold_usd -eq 30) 'Canonical bundle page arrays or editing authority were mistaken for live work.'
$f = Fixture
Reject { Assert-CampaignBillingQuarantine $f.attempt $f.card @($f.bundle) '' } 'Unexplained quarantine was accepted.'
Reject { Assert-CampaignBillingQuarantine $f.attempt $f.card @() 'diagnosed' } 'Missing task evidence was accepted.'
Reject { Assert-NoCampaignNativeProcess 'C:\scenario' @(@{ Name = 'vcp.exe'; CommandLine = 'vcp --data-dir C:\scenario\vcp-data' }) } 'Live native owner was ignored.'
Reject { Assert-NoCampaignNativeProcess 'C:\scenario' @(@{ Name = 'vcp.exe'; CommandLine = 'vcp --data-dir C:/scenario/vcp-data' }) } 'Live native owner using forward slashes was ignored.'
Reject { Assert-NoCampaignNativeProcess 'C:\scenario' @(@{ Name = 'vcp.exe'; CommandLine = $null }) } 'Unknown native owner was ignored.'
Assert-NoCampaignNativeProcess 'C:\scenario' @(@{ Name = 'vcp.exe'; CommandLine = 'vcp --data-dir C:\unrelated' })
$f.attempt.status = 'billing-quarantined'
Reject { Complete-CampaignAttempt $f.attempt $f.card 1 $false } 'Permanent allocation was released by reconciliation.'
$state = @{ schema = 'vcp-ab-campaign/1'; authorized_usd = 100; attempts = @($f.attempt) }
$next = @{ scenario = 'B'; mode = 'Full'; status = 'running'; project = 'C:\old'; liability_usd = 25; fingerprint = 'repaired'; repair_note = 'Changed endpoint; fresh workspace.' }
Reject { Add-CampaignAttempt $state $next } 'Quarantined workspace was reused.'
$next.project = 'C:\old\child'
Reject { Add-CampaignAttempt $state $next } 'Quarantined workspace descendant was reused.'
$next.project = 'C:\fresh'
Add-CampaignAttempt $state $next
Check ((Get-CampaignLiability $state) -eq 55) 'Fresh admission forgot permanently retained liability.'
$next.status = 'accounted'; $next.verdict = 'pass'
$third = @{ scenario = 'A'; mode = 'Full'; status = 'running'; project = 'C:\another'; liability_usd = 46; fingerprint = 'new'; repair_note = '' }
Add-CampaignAttempt $state $third
Check ((Get-CampaignLiability $state) -eq 101) 'Historical hold changed while financial ceiling was suspended.'
$f = Fixture
$f.attempt.cap_usd = 3; $f.attempt.liability_usd = 3; $f.card.max_scenario_usd = 3
$f.bundle.views.costs.items[0].record.cap = '3000000'
$second = $f.bundle | ConvertTo-Json -Depth 30 | ConvertFrom-Json -AsHashtable
$second.task.scope.task = 'task2'; $second.agents.root = 'task2'
foreach ($view in 'tools', 'costs') {
    $second.views[$view].scope.task = 'task2'
    foreach ($item in $second.views[$view].items) { $item.record.scope.task = 'task2' }
}
$f.card.stages += @{ stage = 'T2'; task = 'task2'; skipped = $null; exit_code = 7 }
Reject { Assert-CampaignBillingQuarantine $f.attempt $f.card @($f.bundle, $second) 'diagnosed' } 'Multi-task accounting exceeding the retained attempt allocation was accepted.'
Write-Host "Billing quarantine checks passed: $checks"
