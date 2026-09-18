# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [ValidateSet('Codex', 'Munarium')][string]$Component = 'Codex',
    [ValidateSet('Build', 'BoundaryTests', 'LifecycleTests')][string]$Mode = 'Build',
    [ValidateRange(1, 16)][int]$Jobs = 4,
    [string]$OutputRoot,
    [string]$TargetRoot
)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/build' }
if (-not $TargetRoot) { $TargetRoot = Join-Path $repository 'artifacts/codex-target' }
# Builds committed source directly. Acquisition and patch application are explicit
# maintenance operations; ordinary Cargo dependency provisioning still applies.
$selection = if ($Component -eq 'Munarium') { @{ SelectedMunarium = $true } } else { @{ SelectedCodex = $true } }
& (Join-Path $PSScriptRoot 'upstream/build-baseline.ps1') @selection -OutputRoot $OutputRoot -TargetRoot $TargetRoot -Mode $Mode -Jobs $Jobs
exit $LASTEXITCODE
