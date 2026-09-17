# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [ValidateSet('Build', 'BoundaryTests')][string]$Mode = 'Build',
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
& (Join-Path $PSScriptRoot 'upstream/build-baseline.ps1') -SelectedCodex -OutputRoot $OutputRoot -TargetRoot $TargetRoot -Mode $Mode -Jobs $Jobs
exit $LASTEXITCODE
