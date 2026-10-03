#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
<# Offline bootstrap integration regressions. Native VCP execution is replaced
   inside a temporary harness copy; the real launcher and command logger run. #>
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$scenarioRoot = Split-Path -Parent $PSScriptRoot
$temporaryBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporary = Join-Path $temporaryBase ('vcp-bootstrap-tests-' + [guid]::NewGuid().ToString('N'))
$checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
function New-ProviderFixture([string]$Path, [int]$Hours) {
    New-Item -ItemType Directory -Path (Join-Path $Path 'qualified') -Force | Out-Null
    @{ valid_until = [DateTimeOffset]::UtcNow.AddHours($Hours).ToUnixTimeMilliseconds()
        compatibility = @{ model = 'fixture/model'; endpoint = 'fixture/endpoint' }
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Path 'qualified/snapshot.json')
    '{}' | Set-Content -LiteralPath (Join-Path $Path 'endpoints.json')
}
$savedEnvironment = @{}
foreach ($name in 'LOCALAPPDATA', 'OPENROUTER_API_KEY', 'VCP_DENY_PROVIDER_CREDENTIALS', 'VCP_SCENARIO_CREDENTIAL_ENV', 'VCP_BOOTSTRAP_CASE', 'VCP_BOOTSTRAP_ALIAS', 'VCP_BOOTSTRAP_TEST_KEY') {
    $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
try {
    New-Item -ItemType Directory -Path $temporary | Out-Null
    $fixture = Join-Path $temporary 'launcher fixture'
    New-Item -ItemType Directory -Path $fixture | Out-Null
    Copy-Item -LiteralPath (Join-Path $scenarioRoot 'run-cli-scenarios.ps1') -Destination $fixture
    $harnessPath = Join-Path $scenarioRoot 'VcpScenarioHarness.psm1'
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($harnessPath, [ref]$tokens, [ref]$errors)
    Check ($errors.Count -eq 0) 'Harness has parse errors'
    $native = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Invoke-NativeLogged' }, $true)
    Check ($null -ne $native) 'Cannot isolate native invocation boundary'
    $replacement = @'
function Invoke-NativeLogged {
    param($FilePath, [string[]]$ArgumentList, $WorkingDirectory, $StdoutPath, $StderrPath,
        $TimeoutSeconds, $Environment, $OnLine, $HeartbeatLabel, $Ctx)
    # This mock never starts FilePath. Unexpected command shapes fail closed.
    $setup = [Array]::IndexOf($ArgumentList, 'setup')
    $exitCode = 0
    $data = $null
    if ($setup -ge 0 -and $ArgumentList[$setup + 1] -eq 'credential' -and $ArgumentList[$setup + 2] -eq 'status') {
        $data = @{ environment = $(if ($env:VCP_BOOTSTRAP_ALIAS) { $env:VCP_BOOTSTRAP_ALIAS } else { 'OPENROUTER_API_KEY' }); model_calls = 0 }
    }
    elseif ($ArgumentList[-1] -eq 'models') {
        $data = @{ account = @{ set = @{ roles = @{ main = @('fixture/model') } } } }
    }
    elseif ($setup -ge 0 -and $ArgumentList[$setup + 1] -in 'provider', 'provider-complete') {
        $command = $ArgumentList[$setup + 1]
        $option = if ($command -eq 'provider') { '--output' } else { '--directory' }
        $position = [Array]::IndexOf($ArgumentList, $option)
        if ($position -lt 0) { throw 'Missing generation argument' }
        $generation = $ArgumentList[$position + 1]
        if ($command -eq 'provider') {
            if (Test-Path -LiteralPath $generation) { throw 'Provider output must be new' }
            New-Item -ItemType Directory -Path $generation | Out-Null
            $status = if ($env:VCP_BOOTSTRAP_CASE -eq 'failed') { 'failed' } else { 'observed' }
            @{ status = $status; actual_cost_micros = '12'; ledger = @{ settled = '12'; unresolved = $(if ($status -eq 'failed') { '100' } else { '0' }) } } |
                ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $generation 'result.json')
            '{}' | Set-Content -LiteralPath (Join-Path $generation 'endpoints.json')
        }
        if ($env:VCP_BOOTSTRAP_CASE -eq 'failed' -or
            ($command -eq 'provider' -and $env:VCP_BOOTSTRAP_CASE -in 'delayed', 'receipt-failed') -or
            ($command -eq 'provider-complete' -and $env:VCP_BOOTSTRAP_CASE -eq 'receipt-failed')) {
            $exitCode = 1
            $data = @{ status = 'failed' }
        }
        else {
            New-Item -ItemType Directory -Path (Join-Path $generation 'qualified') | Out-Null
            @{ valid_until = [DateTimeOffset]::UtcNow.AddHours(12).ToUnixTimeMilliseconds()
                compatibility = @{ model = 'fixture/model'; endpoint = 'fixture/endpoint' }
            } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $generation 'qualified/snapshot.json')
            $data = @{ status = 'qualified'; actual_cost_micros = '12'; snapshot = (Join-Path $generation 'qualified/snapshot.json') }
        }
    }
    else { throw ('Unexpected native command: ' + ($ArgumentList -join ' ')) }
    $line = @{ type = 'result'; exit_code = $exitCode; data = $data } | ConvertTo-Json -Depth 20 -Compress
    [IO.File]::WriteAllText($StdoutPath, $line + "`n")
    [IO.File]::WriteAllText($StderrPath, $(if ($exitCode) { 'Synthetic provider failure; evidence retained.' } else { '' }))
    if ($OnLine) { & $OnLine $line }
    return [pscustomobject]@{ ExitCode = $exitCode; TimedOut = $false; DurationSeconds = 0.01 }
}
'@
    $source = [IO.File]::ReadAllText($harnessPath)
    $source = $source.Remove($native.Extent.StartOffset, $native.Extent.EndOffset - $native.Extent.StartOffset).Insert($native.Extent.StartOffset, $replacement)
    [IO.File]::WriteAllText((Join-Path $fixture 'VcpScenarioHarness.psm1'), $source)
    @'
param($ProviderGeneration, $Vcp, $RunRoot, $TurnBudgetUsd, $MaxScenarioUsd, $MaxRepairTurns,
    $OutputTokens, $MaxRequests, $DeadlineSeconds, $ShortDeadlineSeconds, [switch]$SkipPaidStages)
$ErrorActionPreference = 'Stop'
$snapshot = Join-Path $ProviderGeneration 'qualified/snapshot.json'
if (-not (Test-Path -LiteralPath $snapshot -PathType Leaf)) { throw 'Child received unprepared provider' }
$root = Join-Path $RunRoot 'a-vue-taskboard/fixture-run'
foreach ($name in 'results', 'workspace', 'logs', 'vcp-data', 'profiles') {
    New-Item -ItemType Directory -Path (Join-Path $root $name) -Force | Out-Null
}
@{ generation = $ProviderGeneration; credential_name = $env:VCP_SCENARIO_CREDENTIAL_ENV
    credential_available = (-not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($env:VCP_SCENARIO_CREDENTIAL_ENV, 'Process')))
    dry_run = [bool]$SkipPaidStages
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'arguments.json')
@{ verdict = 'fixture-only' } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'results/scorecard.json')
Write-Host 'BOOTSTRAP_CHILD_STARTED'
exit 0
'@ | Set-Content -LiteralPath (Join-Path $fixture 'scenario-a-vue-taskboard.ps1')
    $env:LOCALAPPDATA = Join-Path $temporary 'account data'
    $account = Join-Path $env:LOCALAPPDATA 'VCP/account'
    New-Item -ItemType Directory -Path $account -Force | Out-Null
    @{ version = 1; connection = @{ status = 'connected'; model = 'fixture/model'; endpoint = 'fixture/endpoint' } } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $account 'setup-complete.json')
    $env:OPENROUTER_API_KEY = 'synthetic-bootstrap-key-not-a-real-secret'
    $env:VCP_BOOTSTRAP_TEST_KEY = 'synthetic-custom-bootstrap-key-not-a-real-secret'
    Remove-Item -LiteralPath Env:VCP_DENY_PROVIDER_CREDENTIALS -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath Env:VCP_BOOTSTRAP_ALIAS -ErrorAction SilentlyContinue
    $pwsh = (Get-Process -Id $PID).Path
    function Invoke-BootstrapFixture([string]$Name, [string]$Case, [string]$Generation, [switch]$UseAccountSelection, [string]$Mode = 'Full') {
        $env:VCP_BOOTSTRAP_CASE = $Case
        $runRoot = Join-Path $temporary $Name
        $arguments = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $fixture 'run-cli-scenarios.ps1'),
            '-Scenario', 'A', '-Mode', $Mode, '-ProviderGeneration', $Generation, '-RunRoot', $runRoot, '-Vcp', $pwsh, '-SetupBudgetUsd', '1.25')
        if (-not $UseAccountSelection) { $arguments += @('-Model', 'fixture/model', '-Endpoint', 'fixture/endpoint') }
        $output = (& $pwsh @arguments 2>&1 | Out-String)
        $code = $LASTEXITCODE
        $roots = @(Get-ChildItem -LiteralPath $runRoot -Directory -Filter 'launch-*')
        Check ($roots.Count -eq 1) "Expected one $Name invocation; output: $output"
        $root = $roots[0].FullName
        $audit = Join-Path $root 'setup/logs/vcp-commands.jsonl'
        Check (Test-Path -LiteralPath $audit) "Missing command audit for $Name; output: $output"
        $commands = @(Get-Content -LiteralPath $audit | ForEach-Object { $_ | ConvertFrom-Json -Depth 30 })
        $literal = Get-Content -LiteralPath (Join-Path $root 'setup/logs/vcp-commands.log') -Raw
        Check (-not $literal.Contains('synthetic-bootstrap-key') -and -not $literal.Contains('synthetic-custom-bootstrap-key')) 'Credential material leaked into command log'
        return [pscustomobject]@{ Code = $code; Output = $output; Root = $root; Commands = $commands; Literal = $literal }
    }

    $missing = Join-Path $temporary 'missing-provider'
    $success = Invoke-BootstrapFixture 'missing runs' 'success' $missing
    Check ($success.Code -eq 0 -and $success.Output.Contains('BOOTSTRAP_CHILD_STARTED')) "Missing generation was not prepared: $($success.Output)"
    Check (@($success.Commands | Where-Object label -eq 'setup-provider').Count -eq 1) 'Missing generation did not execute exactly one provider setup'
    $received = Get-Content -LiteralPath (Join-Path $success.Root 'a-vue-taskboard/fixture-run/arguments.json') -Raw | ConvertFrom-Json
    Check ($received.generation -ne $missing -and (Test-Path -LiteralPath (Join-Path $received.generation 'qualified/snapshot.json'))) 'Child did not receive fresh generation'
    Check ($received.credential_available -and $received.credential_name -eq 'OPENROUTER_API_KEY') 'Credential environment was not forwarded'
    foreach ($path in 'setup/results/setup.json', 'setup/results/summary.md', 'setup/logs/vcp-commands.log') {
        Check (Test-Path -LiteralPath (Join-Path $success.Root $path)) "Missing setup evidence $path"
    }
    Check ($success.Literal.Contains('setup') -and $success.Literal.Contains('provider') -and $success.Literal.Contains('results')) 'Literal log omits setup command or evidence pointers'

    # The default workflow and explicit model selections must reuse previous setup.
    $initialGeneration = $received.generation
    $env:VCP_BOOTSTRAP_CASE = 'failed' # A second paid setup would fail this run.
    $reuseRoot = Split-Path -Parent $success.Root
    $reuseOutput = (& $pwsh -NoProfile -NonInteractive -File (Join-Path $fixture 'run-cli-scenarios.ps1') `
        -Scenario A -Mode Full -ProviderGeneration '' -RunRoot $reuseRoot -Vcp $pwsh `
        -Model 'fixture/model' -Endpoint 'fixture/endpoint' -SetupBudgetUsd 1.25 2>&1 | Out-String)
    Check ($LASTEXITCODE -eq 0 -and $reuseOutput.Contains('BOOTSTRAP_CHILD_STARTED')) "Matching cached generation was not reused: $reuseOutput"
    $second = @(Get-ChildItem -LiteralPath $reuseRoot -Directory -Filter 'launch-*' | Where-Object FullName -ne $success.Root)
    Check ($second.Count -eq 1) 'Reuse did not produce exactly one new invocation'
    $reuseCommands = @(Get-Content -LiteralPath (Join-Path $second[0].FullName 'setup/logs/vcp-commands.jsonl') | ForEach-Object { $_ | ConvertFrom-Json })
    Check (@($reuseCommands | Where-Object label -like 'setup-provider*').Count -eq 0) 'Reuse repeated paid qualification'
    $reused = Get-Content -LiteralPath (Join-Path $second[0].FullName 'a-vue-taskboard/fixture-run/arguments.json') -Raw | ConvertFrom-Json
    Check ($reused.generation -eq $initialGeneration) 'Reuse changed the selected provider generation'

    $expired = Join-Path $temporary 'expired-provider'
    New-ProviderFixture $expired -1
    $snapshotBefore = [IO.File]::ReadAllText((Join-Path $expired 'qualified/snapshot.json'))
    $renewed = Invoke-BootstrapFixture 'expired runs' 'success' $expired -UseAccountSelection -Mode DryRun
    Check ($renewed.Code -eq 0 -and $renewed.Output.Contains('BOOTSTRAP_CHILD_STARTED')) "Expired provider was not renewed using account selection: $($renewed.Output)"
    Check ($snapshotBefore -ceq [IO.File]::ReadAllText((Join-Path $expired 'qualified/snapshot.json'))) 'Expired input generation was overwritten'
    $providerCommand = @($renewed.Commands | Where-Object label -eq 'setup-provider')
    Check ($providerCommand.Count -eq 1 -and $providerCommand[0].argv -contains 'fixture/model' -and $providerCommand[0].argv -contains 'fixture/endpoint') 'Installed model/endpoint selection was not used'
    $received = Get-Content -LiteralPath (Join-Path $renewed.Root 'a-vue-taskboard/fixture-run/arguments.json') -Raw | ConvertFrom-Json
    Check $received.dry_run 'DryRun was lost after explicit provider preparation'

    $failed = Invoke-BootstrapFixture 'failed runs' 'failed' $missing
    Check ($failed.Code -eq 1 -and -not $failed.Output.Contains('BOOTSTRAP_CHILD_STARTED')) 'Failed qualification started child or returned success'
    Check (@($failed.Commands | Where-Object label -eq 'setup-provider').Count -eq 1) 'Failed qualification repeated paid setup'
    Check (@($failed.Commands | Where-Object label -eq 'setup-provider-complete').Count -eq 0) 'Unsettled qualification attempted receipt completion'
    $failureEvidence = Get-Content -LiteralPath (Join-Path $failed.Root 'setup/results/setup.json') -Raw | ConvertFrom-Json
    $report = Get-Content -LiteralPath (Join-Path $failureEvidence.generation 'result.json') -Raw | ConvertFrom-Json
    Check ($report.status -eq 'failed' -and $report.ledger.unresolved -eq '100') 'Failed accounting evidence was not preserved'
    Check ($failed.Output.Contains('Setup results:') -and $failed.Output.Contains('vcp-commands.log')) 'Failure output omits evidence locations'

    $delayed = Invoke-BootstrapFixture 'delayed runs' 'delayed' $missing
    Check ($delayed.Code -eq 0 -and $delayed.Output.Contains('BOOTSTRAP_CHILD_STARTED')) "Receipt completion did not resume scenario: $($delayed.Output)"
    Check (@($delayed.Commands | Where-Object label -eq 'setup-provider').Count -eq 1) 'Receipt recovery repeated inference'
    Check (@($delayed.Commands | Where-Object label -eq 'setup-provider-complete').Count -eq 1) 'Receipt recovery did not make exactly one completion attempt'
    $providerRun = @($delayed.Commands | Where-Object label -eq 'setup-provider')[0]
    $completeRun = @($delayed.Commands | Where-Object label -eq 'setup-provider-complete')[0]
    Check ($providerRun.argv[-1] -eq $completeRun.argv[-1]) 'Receipt completion switched evidence directory'

    $receiptFailed = Invoke-BootstrapFixture 'receipt failed runs' 'receipt-failed' $missing
    Check ($receiptFailed.Code -eq 1 -and -not $receiptFailed.Output.Contains('BOOTSTRAP_CHILD_STARTED')) 'Failed receipt completion started a scenario'
    Check (@($receiptFailed.Commands | Where-Object label -eq 'setup-provider').Count -eq 1 -and @($receiptFailed.Commands | Where-Object label -eq 'setup-provider-complete').Count -eq 1) 'Failed receipt completion caused additional retries'

    $env:VCP_BOOTSTRAP_ALIAS = 'VCP_BOOTSTRAP_TEST_KEY'
    $custom = Invoke-BootstrapFixture 'custom credential runs' 'success' $missing
    Check ($custom.Code -eq 0) "Configured environment alias failed: $($custom.Output)"
    $received = Get-Content -LiteralPath (Join-Path $custom.Root 'a-vue-taskboard/fixture-run/arguments.json') -Raw | ConvertFrom-Json
    Check ($received.credential_available -and $received.credential_name -eq 'VCP_BOOTSTRAP_TEST_KEY') 'Custom credential alias was not passed to child'
    Write-Host "Bootstrap regressions passed: $checks checks; no native VCP, network or paid inference executed."
}
finally {
    foreach ($name in $savedEnvironment.Keys) {
        if ($null -eq $savedEnvironment[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
    }
    $resolved = [IO.Path]::GetFullPath($temporary)
    $basePrefix = $temporaryBase.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if ($resolved.StartsWith($basePrefix, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $resolved) -like 'vcp-bootstrap-tests-*') {
        if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
    }
}
