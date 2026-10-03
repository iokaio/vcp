#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Exercise Scenario A's actual profile composition without native VCP or inference.
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$source = Get-Content -LiteralPath (Join-Path $scenarioRoot 'scenario-a-vue-taskboard.ps1') -Raw
$start = $source.IndexOf("    `$stage = 'P1-profiles'")
$end = $source.IndexOf('    foreach ($key in', $start)
Assert-That ($start -ge 0 -and $end -gt $start) 'Scenario A profile block not found'
$compose = [scriptblock]::Create($source.Substring($start, $end - $start))
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase ('vcp-profile-deadlines-' + [guid]::NewGuid().ToString('N'))
$node = (Get-Process -Id $PID).Path
$namesT1 = @('health')
$namesT3 = @('health', 'create')
$namesT4 = @('health', 'create', 'regression')
$namesT5 = @('health', 'create', 'regression', 'production')
$checks = 0
try {
    New-Item -ItemType Directory -Path $root | Out-Null
    foreach ($limits in @(@(1800, 150), @(60, 30), @(300, 300), @(900, 600), @(1, 1))) {
        $ctx = @{
            Workspace = $root; Profiles = $root; Temp = $root; RunId = "$($limits[0])-$($limits[1])"
            Catalog = (Join-Path $root 'endpoints.json'); SnapshotText = '{}'
            TurnBudgetUsd = [decimal]3; OutputTokens = 8192; MaxRequests = 96
            DeadlineSeconds = $limits[0]; ShortDeadlineSeconds = $limits[1]
        }
        . $compose
        foreach ($name in 'T1', 'T2', 'T3', 'T4', 'T5', 'T5-short') {
            $profile = Get-Content -LiteralPath $profiles[$name] -Raw | ConvertFrom-Json -Depth 100
            $deadline = if ($name -eq 'T5-short') { $limits[1] } else { $limits[0] }
            Assert-That ($profile.deadline_seconds -eq $deadline) "$name changed the requested task deadline"
            Assert-That ($profile.checks.Count -eq 1) "$name lost its required verification check"
            $check = $profile.checks[0]
            Assert-That ($check.timeout_ms -gt 0 -and $check.timeout_ms -le $deadline * 1000) "$name check exceeds task deadline"
            Assert-That ($check.timeout_ms -le $profile.processes[0].max_timeout_ms) "$name check exceeds process ceiling"
            Assert-That ($check.timeout_ms -eq [math]::Min(300000, $deadline * 1000)) "$name changed the normal five-minute check ceiling"
            $expected = if ($name -in 'T5', 'T5-short') { $namesT5 } elseif ($name -eq 'T4') { $namesT4 } elseif ($name -eq 'T3') { $namesT3 } else { $namesT1 }
            Assert-That (($check.expected_tests -join ',') -ceq ($expected -join ',')) "$name lost required named tests"
            $checks += 6
        }
    }
    Write-Host "Profile deadline regressions passed: $checks checks; no VCP or inference executed."
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    $prefix = $tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if ($resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $resolved) -like 'vcp-profile-deadlines-*') {
        Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction SilentlyContinue
    }
}
