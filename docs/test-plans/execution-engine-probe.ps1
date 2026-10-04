#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# EE-06: a real repair through the existing driver, with independent quality gates.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Vcp,
    [Parameter(Mandatory)][string]$ProviderGeneration,
    [string]$RunRoot = 'C:\vcp-scenarios\execution-engine-probes',
    [string]$ProjectPath = ('D:\clitests\execution-engine-probe-' + [guid]::NewGuid().ToString('N')),
    [switch]$SkipPaidStages,
    [switch]$PauseAfterProgress,
    [switch]$PauseAtCheckpoint
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$ctx = Initialize-VcpScenario -Name 'ee06-cart-repair' -RunRoot $RunRoot -ProjectPath $ProjectPath -Vcp $Vcp -ProviderGeneration $ProviderGeneration -AllowProcessPublish -SkipPaidStages:$SkipPaidStages
try {
    Assert-That (-not $ctx.ReuseProject) 'The diagnostic probe requires a fresh empty project; retained runs must not be overwritten.'
    $ctx.Notes.Add('EE-06 small real repair only. This is not full A/B, larger-engagement, unknown-price admission or live history-eviction qualification.')
    $seed = @{
        'package.json' = '{"name":"execution-engine-cart-probe","version":"1.0.0","private":true,"type":"module","scripts":{"test":"node --test cart.test.js"}}'
        'cart.js' = 'export function totalCents(items) { return items.reduce((sum, item) => sum + item.priceCents, 0); }'
        'cart.test.js' = @'
import test from 'node:test';
import assert from 'node:assert/strict';
import { totalCents } from './cart.js';
test('empty cart', () => assert.equal(totalCents([]), 0));
test('quantity multiplication', () => assert.equal(totalCents([{priceCents:125,quantity:3},{priceCents:49,quantity:2}]), 473));
test('zero quantity', () => assert.equal(totalCents([{priceCents:125,quantity:0}]), 0));
test('invalid inputs', () => {
  for (const input of [null, {}, [null], [{priceCents:-1,quantity:2}], [{priceCents:2,quantity:1.5}], [{priceCents:2,quantity:-1}], [{priceCents:NaN,quantity:1}], [{priceCents:Infinity,quantity:1}], [{priceCents:2}]]) assert.throws(() => totalCents(input));
});
test('safe integer overflow', () => {
  assert.throws(() => totalCents([{priceCents:Number.MAX_SAFE_INTEGER,quantity:2}]));
  assert.throws(() => totalCents([{priceCents:Number.MAX_SAFE_INTEGER,quantity:1},{priceCents:1,quantity:1}]));
});
test('inputs unchanged', () => {
  const items = Object.freeze([Object.freeze({priceCents:12,quantity:4})]);
  assert.equal(totalCents(items),48);
});
'@
        'README.md' = 'Repair totalCents in cart.js. Accept an array of objects with nonnegative safe integer priceCents and quantity. Return their exact sum of products in integer cents; reject invalid input and unsafe products or totals. Do not mutate input. Keep the public export and the tests unchanged.'
    }
    Write-SeedFiles $ctx.Workspace $seed
    $protected = @{}
    foreach ($name in 'cart.test.js','package.json','README.md') { $protected[$name] = Get-Sha256 (Join-Path $ctx.Workspace $name) }
    $checkpointPrompt = ''
    if ($PauseAtCheckpoint) {
        Assert-That (-not $PauseAfterProgress) 'Choose the arbitrary-progress probe or the recorded checkpoint, not both.'
        $checkpointPrompt = New-ScenarioPauseCheckpoint $ctx
        $protected[(Split-Path -Leaf $ctx.PauseCheckpoint.script)] = $ctx.PauseCheckpoint.sha256
        $PauseAfterProgress = $true
    }
    Initialize-GitCheckpoint $ctx
    $node = Find-Executable -Name 'node'
    Assert-That ([bool]$node) 'Node is required for independent acceptance checks.'
    $baseline = Invoke-Tool -Ctx $ctx -Stage 'baseline' -Label 'seeded-failure' -FilePath $node -ArgumentList @('--test','cart.test.js') -TimeoutSeconds 60
    [void](Invoke-Gate -Ctx $ctx -Stage 'B0-baseline' -Id 'seed-fails' -Description 'the original implementation fails the specified behavior' -Test { Assert-That ($baseline.ExitCode -ne 0) 'Seed unexpectedly passed'; $true })
    Invoke-CommonPreflight $ctx
    $process = New-ProcessProfile -Name 'node' -Executable $node -Ctx $ctx -MaxTimeoutMs 300000
    $check = @{manifest='package.json';runner='node';profile='node';timeout_ms=300000;expected_tests=@('empty cart','quantity multiplication','zero quantity','invalid inputs','safe integer overflow','inputs unchanged');rationale='Owner acceptance: exact integer cart arithmetic and input validation, without changing protected tests.'}
    $profile = New-ScenarioProfile -Ctx $ctx -Name 'repair' -AffectedPaths @('cart.js') -Processes @($process) -Checks @($check)
    [void](Test-ProfileCheck $ctx 'P1-profile' $profile 'repair')
    $accepted = if ($PauseAfterProgress) { @(8) } else { @(0) }
    $result = Invoke-VcpTask -Ctx $ctx -Stage 'repair' -Title 'Repair integer cart arithmetic' -Config $profile -PauseAfterProgress:$PauseAfterProgress -AcceptExit $accepted -Prompt ($checkpointPrompt + ' Implement the requirements in README.md by changing only cart.js. Preserve tests and package configuration. Use the configured verification check to confirm all six required tests pass; repair any failure before finishing.')
    if ($result) {
        Test-StageExit $ctx $result 'repair'
        if ($PauseAfterProgress) {
            [void](Invoke-Gate $ctx 'repair' 'explicit-pause' 'owner acknowledged pause and execution ended durably paused' {
                Assert-That ($result.explicit_pause.acknowledged -and $result.exit_code -eq 8 -and $result.conditions -contains 'durably_paused' -and $ctx.PaidExecutionBlock.resume_same_task) 'Explicit pause lacks acknowledged, scoped terminal proof'; $true
            })
            if (@(Get-FailedGates $ctx 'repair').Count) { throw 'Explicit pause qualification failed; inspect retained evidence before continuation.' }
            $resumed = Invoke-VcpContinuation -Ctx $ctx -Stage 'resume' -Title 'Resume explicitly paused cart repair' -Arguments @('resume',$result.task) -Config $profile
            Assert-That ($null -ne $resumed) 'Same-task continuation did not run'
            Test-StageExit $ctx $resumed 'resume'
            [void](Invoke-Gate $ctx 'resume' 'same-task' 'resume continued the explicitly paused task' { Assert-That ($resumed.task -eq $result.task) 'Different task resumed'; $true })
        }
        [void](Invoke-Gate -Ctx $ctx -Stage 'FINAL-quality' -Id 'protected-files' -Description 'protected acceptance files are unchanged' -Test {
            foreach ($name in $protected.Keys) { Assert-That ((Get-Sha256 (Join-Path $ctx.Workspace $name)) -ceq $protected[$name]) "Protected file changed: $name" }; $true
        })
        [void](Invoke-Gate -Ctx $ctx -Stage 'FINAL-quality' -Id 'independent-tests' -Description 'all owner acceptance tests pass independently of model reporting' -Test {
            $run = Invoke-Tool -Ctx $ctx -Stage 'FINAL-quality' -Label 'independent-node-tests' -FilePath $node -ArgumentList @('--test','cart.test.js') -TimeoutSeconds 60
            Assert-That ($run.ExitCode -eq 0 -and -not $run.TimedOut) 'Independent acceptance failed'; $true
        })
        Save-Checkpoint $ctx 'EE-06: retain observed cart repair'
    }
} catch {
    $ctx.Fatal = $_.Exception.Message
    $ctx.Notes.Add($ctx.Fatal)
    Write-Step $ctx $ctx.Fatal 'fail'
} finally {
    $exitCode = Complete-VcpScenario $ctx
}
exit $exitCode
