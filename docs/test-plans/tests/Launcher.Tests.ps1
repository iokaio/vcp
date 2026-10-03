#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
<# Offline launcher tests. The only child scenario is a local fixture; VCP is never executed. #>
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$scenarioRoot = Split-Path -Parent $PSScriptRoot
$launcher = Join-Path $scenarioRoot 'run-cli-scenarios.ps1'
$temporaryBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporary = Join-Path $temporaryBase ('vcp-launcher-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
$checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
function Import-LauncherFunction([string]$Name) {
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($launcher, [ref]$tokens, [ref]$errors)
    Check ($errors.Count -eq 0) "Launcher parsing failed: $errors"
    $definition = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $Name }, $true)
    Check ($null -ne $definition) "Missing helper $Name"
    . ([scriptblock]::Create(($definition.Extent.Text -replace ('^function\s+' + [regex]::Escape($Name)), "function script:$Name")))
}
function New-ProviderFixture([string]$Path, [int]$Hours) {
    New-Item -ItemType Directory -Path (Join-Path $Path 'qualified') -Force | Out-Null
    @{ valid_until = [DateTimeOffset]::UtcNow.AddHours($Hours).ToUnixTimeMilliseconds() } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Path 'qualified/snapshot.json')
    '{}' | Set-Content -LiteralPath (Join-Path $Path 'endpoints.json')
}
$originalCulture = [Globalization.CultureInfo]::CurrentCulture
$originalVcpEnv = $env:VCP_EXE
try {
    foreach ($name in 'Read-LauncherChoice', 'Assert-LauncherProvider', 'Find-LauncherProvider', 'New-LauncherArguments') { Import-LauncherFunction $name }
    $script:answers = [Collections.Generic.Queue[string]]::new()
    $script:answers.Enqueue('wrong'); $script:answers.Enqueue('b')
    function Read-Host { param($Prompt) $script:answers.Dequeue() }
    Check ((Read-LauncherChoice 'scenario' @('A', 'B', 'C', 'D')) -eq 'B') 'Menu did not retry invalid input or normalize case'
    $script:answers.Enqueue('')
    Check ((Read-LauncherChoice 'mode' @('DryRun', 'Full') 'DryRun') -eq 'DryRun') 'Mode did not default to DryRun'

    $profiles = Join-Path $temporary 'profiles'
    $generation = Join-Path $profiles 'provider-valid with spaces'
    New-ProviderFixture $generation 12
    $stale = Join-Path $profiles 'provider-expiring'
    New-ProviderFixture $stale 1
    Check ((Assert-LauncherProvider $generation) -eq $generation) 'Valid generation rejected'
    Check ((Find-LauncherProvider $profiles) -eq $generation) 'Discovery did not skip a newer expiring generation'
    $rejected = $false
    try { Assert-LauncherProvider $stale | Out-Null } catch { $rejected = $true }
    Check $rejected 'Expiring generation accepted'
    $rejected = $false
    try { Assert-LauncherProvider (Join-Path $temporary 'missing') | Out-Null } catch { $rejected = $true }
    Check $rejected 'Missing generation accepted'

    [Globalization.CultureInfo]::CurrentCulture = [Globalization.CultureInfo]::GetCultureInfo('fr-FR')
    $arguments = @(New-LauncherArguments 'C:\path with spaces\scenario.ps1' @{ TurnBudgetUsd = [decimal]1.25; ProviderGeneration = $generation } $true)
    Check ($arguments[3] -eq 'C:\path with spaces\scenario.ps1') 'Script path was split or quoted into literal argument text'
    Check ($arguments -contains '1.25' -and $arguments -contains $generation -and $arguments -contains '-SkipPaidStages') 'Arguments lost invariant decimals, provider path, or dry-run flag'
    Check (-not (@(New-LauncherArguments 'fixture.ps1' @{} $false) -contains '-SkipPaidStages')) 'Full argument construction unexpectedly added dry-run flag'
    [Globalization.CultureInfo]::CurrentCulture = $originalCulture

    # Copy the launcher next to an inert scenario fixture and the real harness.
    # This exercises executable resolution, native argv/console output, exit-code
    # propagation and result discovery without launching an installed VCP binary.
    $fixture = Join-Path $temporary 'launcher fixture'
    New-Item -ItemType Directory -Path $fixture | Out-Null
    Copy-Item -LiteralPath $launcher -Destination $fixture
    Copy-Item -LiteralPath (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Destination $fixture
    @'
param($ProviderGeneration, $Vcp, $RunRoot, [decimal]$TurnBudgetUsd, $MaxScenarioUsd, $MaxRepairTurns,
    $OutputTokens, $MaxRequests, $DeadlineSeconds, $ShortDeadlineSeconds, [switch]$SkipPaidStages)
$ErrorActionPreference = 'Stop'
if (-not $SkipPaidStages) { throw 'Fixture permits DryRun only.' }
$root = Join-Path $RunRoot 'a-vue-taskboard/fixture-run'
foreach ($name in 'results', 'workspace', 'logs', 'vcp-data', 'profiles') {
    New-Item -ItemType Directory -Path (Join-Path $root $name) -Force | Out-Null
}
@{ verdict = 'dry-run-fail' } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'results/scorecard.json')
@{ generation = $ProviderGeneration; budget = $TurnBudgetUsd } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'arguments.json')
Write-Output 'FIXTURE_STDOUT_PROGRESS'
Write-Host 'FIXTURE_HOST_PROGRESS'
[Console]::Error.WriteLine('FIXTURE_STDERR_PROGRESS')
exit 7
'@ | Set-Content -LiteralPath (Join-Path $fixture 'scenario-a-vue-taskboard.ps1')
    $pwsh = (Get-Process -Id $PID).Path
    $runRoot = Join-Path $temporary 'runs with spaces'
    $output = (& $pwsh -NoProfile -NonInteractive -File (Join-Path $fixture 'run-cli-scenarios.ps1') -Scenario A -Mode DryRun -ProviderGeneration $generation -RunRoot $runRoot -Vcp $pwsh -TurnBudgetUsd 1.25 2>&1 | Out-String)
    $code = $LASTEXITCODE
    Check ($code -eq 7) "Child exit 7 became $code; output: $output"
    foreach ($marker in 'FIXTURE_STDOUT_PROGRESS', 'FIXTURE_HOST_PROGRESS', 'FIXTURE_STDERR_PROGRESS', 'Workspace:', 'Results:', 'Logs:', 'VCP data:', 'Profiles:', 'vcp-commands.log', 'vcp-commands.jsonl', 'dry-run-fail') {
        Check ($output.Contains($marker)) "Console output missing $marker"
    }
    $invocations = @(Get-ChildItem -LiteralPath $runRoot -Directory -Filter 'launch-*')
    Check ($invocations.Count -eq 1) 'Launcher did not isolate its run in one invocation folder'
    $received = Get-Content -LiteralPath (Join-Path $invocations[0].FullName 'a-vue-taskboard/fixture-run/arguments.json') -Raw | ConvertFrom-Json
    Check ($received.generation -eq $generation -and $received.budget -eq 1.25) 'Child received corrupted arguments'

    $badOutput = (& $pwsh -NoProfile -NonInteractive -File (Join-Path $fixture 'run-cli-scenarios.ps1') -Scenario A -Mode DryRun -ProviderGeneration $generation -RunRoot $runRoot -Vcp (Join-Path $temporary 'missing-vcp.exe') 2>&1 | Out-String)
    Check ($LASTEXITCODE -eq 1 -and $badOutput.Contains('Requested VCP executable does not exist')) 'Invalid explicit VCP path silently fell back to another executable'
    $env:VCP_EXE = Join-Path $temporary 'missing-env-vcp.exe'
    $badOutput = (& $pwsh -NoProfile -NonInteractive -File (Join-Path $fixture 'run-cli-scenarios.ps1') -Scenario A -Mode DryRun -ProviderGeneration $generation -RunRoot $runRoot 2>&1 | Out-String)
    Check ($LASTEXITCODE -eq 1 -and $badOutput.Contains('VCP_EXE does not exist')) 'Invalid VCP_EXE silently fell back to another executable'
    Check (@(Get-ChildItem -LiteralPath $runRoot -Directory).Count -eq 1) 'Validation failure started a scenario'
    Write-Host "Launcher regressions passed: $checks checks; no VCP or paid scenario executed."
}
finally {
    [Globalization.CultureInfo]::CurrentCulture = $originalCulture
    [Environment]::SetEnvironmentVariable('VCP_EXE', $originalVcpEnv, 'Process')
    $resolved = [IO.Path]::GetFullPath($temporary)
    if ($resolved.StartsWith($temporaryBase, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $resolved) -like 'vcp-launcher-tests-*') {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
