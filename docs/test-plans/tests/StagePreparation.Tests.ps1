#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Real admission/profile/evidence functions with an inert native boundary.
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase ('vcp-stage-preparation-' + [guid]::NewGuid().ToString('N'))
$checks = 0
function Check([bool]$Pass, [string]$Message) { if (-not $Pass) { throw $Message }; $script:checks++ }
try {
    New-Item -ItemType Directory -Path $root | Out-Null
    $ctx = @{ Root = $root; Profiles = $root; Logs = $root; Name = 'preparation'; ProgressLog = Join-Path $root 'progress.log';
        Gates = [Collections.Generic.List[object]]::new(); Notes = [Collections.Generic.List[string]]::new() }
    $catalog = Join-Path $root 'original-catalog.json'
    Write-Utf8File $catalog '{}'
    $snapshot = @{ valid_until = [DateTimeOffset]::UtcNow.AddMinutes(2).ToUnixTimeMilliseconds(); compatibility = @{ model = 'fixture/model'; endpoint = 'fixture/endpoint' } }
    $profile = Join-Path $root 'profile.json'
    Write-JsonFile $profile @{ provider = $snapshot; catalog = $catalog; deadline_seconds = 600; automatic_effects = @('read'); budget_usd = '3.00' }
    $originalHash = (Get-FileHash $profile).Hash
    & $module {
        $script:prepCalls = [Collections.Generic.List[object]]::new()
        $script:prepMode = 'success'
        function script:Invoke-Vcp {
            param($Ctx, $Stage, $Label, [string[]]$Arguments, $Config, [switch]$DenyProviderCredentials, $TimeoutSeconds)
            $script:prepCalls.Add(@{ label = $Label; arguments = $Arguments; denied_credentials = [bool]$DenyProviderCredentials })
            $data = @{ status = 'refreshed'; model_calls = 0 }; $code = 0
            if ($Label -eq 'provider-refresh') {
                $output = $Arguments[-1]
                New-Item -ItemType Directory -Path $output | Out-Null
                Write-Utf8File (Join-Path $output 'endpoints.json') '{}'
                $endpoint = if ($script:prepMode -eq 'wrong-endpoint') { 'wrong/endpoint' } else { 'fixture/endpoint' }
                Write-JsonFile (Join-Path $output 'snapshot.json') @{
                    valid_until = [DateTimeOffset]::UtcNow.AddHours(12).ToUnixTimeMilliseconds()
                    raw_sha256 = (Get-FileHash (Join-Path $output 'endpoints.json')).Hash
                    compatibility = @{ model = 'fixture/model'; endpoint = $endpoint }
                }
            }
            elseif ($Label -eq 'setup-check-refreshed' -and $script:prepMode -eq 'check-fails') { $code = 2 }
            elseif ($Label -eq 'inspect-bundle') {
                $views = @{}
                foreach ($view in 'costs', 'verification', 'tools', 'routing', 'policy', 'outputs') {
                    $items = if ($view -eq 'costs') { @(@{ collection = 'ledger'; record = @{ settled = '331439'; active = '0'; unresolved = '0' } }) } else { @() }
                    $views[$view] = @(@{ view = $view; scope = @{ task = 'task-fixture' }; source_watermark = '10'; items = $items; gaps = @(); next_cursor = $null })
                }
                if ($script:prepMode -eq 'partial') { $views.costs[0].next_cursor = 'more' }
                if ($script:prepMode -eq 'wrong-task') { $views.costs[0].scope.task = 'other-task' }
                if ($script:prepMode -eq 'wrong-watermark') { $views.costs[0].source_watermark = '11' }
                $data = @{ schema_version = 1; source_watermark = '10'; task = @{ scope = @{ task = 'task-fixture' } }; views = $views; agents = @(); history = @() }
            }
            return [pscustomobject]@{ ExitCode = $code; TimedOut = $false; InvalidLines = 0; Result = @{ data = $data }; StderrPath = 'fixture.stderr' }
        }
    }
    $fresh = & $module { param($c, $p) Get-FreshScenarioProfile $c 'T2' $p } $ctx $profile
    $document = Get-Content $fresh -Raw | ConvertFrom-Json
    Check ($fresh -ne $profile -and (Get-FileHash $profile).Hash -eq $originalHash) 'Refresh overwrote original profile'
    Check ($document.automatic_effects.Count -eq 1 -and $document.automatic_effects[0] -eq 'read' -and $document.budget_usd -eq '3.00') 'Refresh changed authorization or budget'
    $calls = & $module { @($script:prepCalls) }
    Check ($calls.Count -eq 2 -and @($calls | Where-Object { -not $_.denied_credentials }).Count -eq 0) 'Refresh used inference credentials or unexpected calls'
    $second = & $module { param($c, $p) Get-FreshScenarioProfile $c 'T3' $p } $ctx $profile
    $calls = & $module { @($script:prepCalls) }
    Check (@($calls | Where-Object label -eq 'provider-refresh').Count -eq 1) 'Another stage refetched still-current metadata'
    Check ($second -ne $fresh -and (Test-Path $fresh)) 'Another stage replaced retained profile evidence'
    $same = & $module { param($c, $p) Get-FreshScenarioProfile $c 'T4' $p } $ctx $fresh
    Check ($same -eq $fresh) 'Current profile unnecessarily replaced'
    foreach ($mode in 'wrong-endpoint', 'check-fails') {
        $ctx.SnapshotText = $null
        & $module { param($mode) $script:prepMode = $mode } $mode
        $rejected = $false
        try { & $module { param($c, $p) Get-FreshScenarioProfile $c 'rejected' $p } $ctx $profile | Out-Null } catch { $rejected = $true }
        Check $rejected "$mode admitted a task with invalid refreshed metadata"
    }
    foreach ($mode in 'success', 'partial', 'wrong-task', 'wrong-watermark') {
        & $module { param($mode) $script:prepMode = $mode; $script:prepCalls.Clear() } $mode
        $ctx.Gates.Clear()
        $views = & $module { param($c) Get-VcpStageInspection $c 'inspect' 'task-fixture' } $ctx
        $cost = Get-VcpTaskCost $views.costs
        Check ((& $module { $script:prepCalls.Count }) -eq 1) 'Bundle made repeated native evidence calls'
        if ($mode -eq 'success') {
            Check ($cost.Usd -eq [decimal]0.331439 -and @($ctx.Gates | Where-Object outcome -ne pass).Count -eq 0) 'Complete bundle failed evidence checks'
        }
        else { Check ($null -eq $cost.Usd -and @($ctx.Gates | Where-Object outcome -eq fail).Count -gt 0) "$mode evidence silently passed accounting" }
    }
    Write-Host "PASS: $checks offline stage preparation and bundled inspection checks"
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    if (-not $resolved.StartsWith($tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $resolved -Leaf) -notlike 'vcp-stage-preparation-*') { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
