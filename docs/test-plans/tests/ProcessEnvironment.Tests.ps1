#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Run real child processes through the profile-environment preflight. No VCP or inference.
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -DisableNameChecking -PassThru
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase ('vcp-process-environment-' + [guid]::NewGuid().ToString('N'))
$saved = $env:VCP_BOOTSTRAP_TEST_AMBIENT
try {
    New-Item -ItemType Directory -Path $root | Out-Null
    $ctx = @{
        Workspace = $root; Temp = $root; Logs = (Join-Path $root 'logs'); Name = 'process-environment'
        ProgressLog = (Join-Path $root 'progress.log'); ToolLog = (Join-Path $root 'tools.log')
        Gates = [Collections.Generic.List[object]]::new()
        Stages = [Collections.Generic.List[object]]::new()
        TurnBudgetUsd = [decimal]3; MaxScenarioUsd = [decimal]30; SpentUsd = [decimal]0
        SkipPaidStages = $false; PaidExecutionBlock = $null
    }
    $probe = Join-Path $root 'probe.ps1'
    Write-Utf8File $probe @'
param([string]$Mode)
if ($env:VCP_BOOTSTRAP_TEST_AMBIENT) { throw 'Ambient environment leaked into process profile' }
if ($env:CI -ne 'true') { throw 'Profile environment missing' }
if ($env:TEMP -ne $PSScriptRoot) { throw 'Profile temporary directory changed' }
if ($Mode -eq 'fail') { [Console]::Error.WriteLine('toolchain-specific failure'); exit 23 }
if ($Mode -eq 'timeout') { Start-Sleep -Seconds 20 }
'profile bootstrap passed'
'@
    $env:VCP_BOOTSTRAP_TEST_AMBIENT = 'must-not-inherit'
    $process = New-ProcessProfile -Name 'fixture' -Executable (Get-Process -Id $PID).Path -Ctx $ctx -MaxTimeoutMs 10000
    $pass = Test-ProcessEnvironment $ctx 'P1-profiles' $process 'pass' @('-NoProfile', '-File', $probe, 'pass')
    Assert-That ($pass.required -and $pass.outcome -eq 'pass') 'Real scrubbed process failed'
    $fail = Test-ProcessEnvironment $ctx 'P1-profiles' $process 'fail' @('-NoProfile', '-File', $probe, 'fail')
    Assert-That ($fail.required -and $fail.outcome -eq 'fail' -and $fail.detail -like '*23*toolchain-specific failure*') 'Toolchain failure was hidden'
    # Exercise real paid admission with the failed process gate. Only the CLI
    # dispatch boundary is inert; an accidental paid call cannot reach a provider.
    & $module {
        $script:paidProbeCalls = 0
        function script:Invoke-Vcp {
            param($Ctx, $Stage, $Label, $Config, [string[]]$Arguments, $TimeoutSeconds, [switch]$Live)
            $script:paidProbeCalls++
            throw 'Unexpected CLI invocation after failed process preflight'
        }
    }
    $admissionError = $null
    try { [void](Invoke-VcpTask $ctx 'T1' 'Must not start' 'No inference permitted.' 'missing-profile.json') }
    catch { $admissionError = $_.Exception.Message }
    Assert-That ($admissionError -eq 'Required preflight, baseline, profile or guardrail checks failed; refusing paid execution.') "Failed process gate did not refuse paid admission: $admissionError"
    Assert-That ((& $module { $script:paidProbeCalls }) -eq 0) 'Failed process preflight reached the CLI dispatch boundary'
    Assert-That ($ctx.Stages.Count -eq 0 -and $ctx.SpentUsd -eq 0) 'Rejected paid admission changed task or spend records'
    $before = @(Get-ChildItem -LiteralPath $ctx.Logs -Recurse -File).Count
    $skip = Test-ProcessEnvironment $ctx 'P1-profiles' $process 'blocked' @('-NoProfile', '-File', 'missing.ps1')
    Assert-That ($skip.outcome -eq 'skip' -and @(Get-ChildItem -LiteralPath $ctx.Logs -Recurse -File).Count -eq $before) 'Failed preflight continued spawning processes'
    $process.max_timeout_ms = 1000
    $timeout = Test-ProcessEnvironment $ctx 'P1-timeout' $process 'timeout' @('-NoProfile', '-File', $probe, 'timeout')
    Assert-That ($timeout.required -and $timeout.outcome -eq 'fail' -and $timeout.detail -like '*exit -1*') 'Timed-out process passed preflight'
    Assert-That ($env:VCP_BOOTSTRAP_TEST_AMBIENT -eq 'must-not-inherit') 'Preflight changed parent environment'
    Write-Host 'Process environment preflight passed: real cleared environment, preserved parent, failure evidence, refused paid admission, stop-after-failure and timeout.'
}
finally {
    Remove-Module $module -Force
    $env:VCP_BOOTSTRAP_TEST_AMBIENT = $saved
    $resolved = [IO.Path]::GetFullPath($root)
    $prefix = $tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notlike 'vcp-process-environment-*') { throw 'Unsafe cleanup path' }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
