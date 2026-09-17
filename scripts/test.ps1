# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [string]$Suite = 'fast',
    [string]$Case,
    [string]$Backend,
    [string]$OutputRoot
)
$ErrorActionPreference = 'Stop'
$nodeCommand = Get-Command node -CommandType Application -ErrorAction SilentlyContinue
if (-not $nodeCommand) {
    Write-Error 'Node.js 24 is required for the delivery harness; tests were not run.' -ErrorAction Continue
    exit 3
}
$runnerArguments = @((Join-Path $PSScriptRoot 'test-runner.cjs'), '--suite', $Suite)
if ($PSBoundParameters.ContainsKey('Case')) { $runnerArguments += @('--case', $Case) }
if ($PSBoundParameters.ContainsKey('Backend')) { $runnerArguments += @('--backend', $Backend) }
if ($PSBoundParameters.ContainsKey('OutputRoot')) { $runnerArguments += @('--output-root', $OutputRoot) }
& $nodeCommand.Source @runnerArguments
exit $LASTEXITCODE
