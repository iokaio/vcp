#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Offline integration: real launcher/logger, inert native VCP and scenario fixtures.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$scenarioRoot = Split-Path -Parent $PSScriptRoot
$temporaryBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporary = Join-Path $temporaryBase ('vcp-provider-reuse-tests-' + [guid]::NewGuid().ToString('N'))
$checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
function New-ProviderFixture([string]$Path, [int]$Minutes = 45, [string]$Model = 'fixture/model') {
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $Path 'endpoints.json'), '{"data":{"fixture":true}}')
    @{ valid_until = [DateTimeOffset]::UtcNow.AddMinutes($Minutes).ToUnixTimeMilliseconds()
        raw_sha256 = (Get-FileHash -LiteralPath (Join-Path $Path 'endpoints.json')).Hash.ToLowerInvariant()
        compatibility = @{ model = $Model; endpoint = 'fixture/endpoint' }; max_output = '8192'
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Path 'snapshot.json')
}
$savedEnvironment = @{}
foreach ($name in 'LOCALAPPDATA', 'VCP_PROVIDER_GENERATION', 'VCP_DENY_PROVIDER_CREDENTIALS', 'VCP_SCENARIO_CREDENTIAL_ENV', 'VCP_REUSE_MODEL', 'VCP_REUSE_KEY') {
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
    # Unexpected configuration or qualification fails immediately.
    if ($ArgumentList[-1] -eq 'models') {
        $data = @{ effective = @{ set = @{ roles = @{ main = @($env:VCP_REUSE_MODEL) } } }
            account = @{ set = @{ roles = @{ main = @('wrong/account-default') } } } }
    }
    elseif (($ArgumentList[-3..-1] -join ' ') -eq 'setup credential status') {
        $data = @{ environment = 'VCP_REUSE_KEY'; model_calls = 0 }
    }
    else { throw ('Unexpected VCP command: ' + ($ArgumentList -join ' ')) }
    $line = @{ type = 'result'; exit_code = 0; data = $data } | ConvertTo-Json -Depth 20 -Compress
    [IO.File]::WriteAllText($StdoutPath, $line + "`n")
    [IO.File]::WriteAllText($StderrPath, '')
    if ($OnLine) { & $OnLine $line }
    return [pscustomobject]@{ ExitCode = 0; TimedOut = $false; DurationSeconds = 0.01 }
}
'@
    $source = [IO.File]::ReadAllText($harnessPath)
    $source = $source.Remove($native.Extent.StartOffset, $native.Extent.EndOffset - $native.Extent.StartOffset).Insert($native.Extent.StartOffset, $replacement)
    [IO.File]::WriteAllText((Join-Path $fixture 'VcpScenarioHarness.psm1'), $source)
    @'
param($ProviderGeneration, $Vcp, $RunRoot, $ProjectPath, $TurnBudgetUsd, $MaxScenarioUsd, $MaxRepairTurns,
    $OutputTokens, $MaxRequests, $DeadlineSeconds, $ShortDeadlineSeconds, [switch]$SkipPaidStages, [switch]$AllowProcessPublish)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath (Join-Path $ProviderGeneration 'snapshot.json'))) { throw 'Missing retained metadata' }
$root = Join-Path $RunRoot 'a-vue-taskboard/fixture-run'
foreach ($name in 'results', 'logs', 'vcp-data', 'profiles') { New-Item -ItemType Directory -Path (Join-Path $root $name) -Force | Out-Null }
@{ generation = $ProviderGeneration; credential_name = $env:VCP_SCENARIO_CREDENTIAL_ENV
    project = $ProjectPath; project_exists = (Test-Path -LiteralPath $ProjectPath -PathType Container)
    dry_run = [bool]$SkipPaidStages; allow_process_publish = [bool]$AllowProcessPublish
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'arguments.json')
@{ verdict = 'fixture-only' } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'results/scorecard.json')
Write-Host 'PROVIDER_REUSE_CHILD_STARTED'
exit 0
'@ | Set-Content -LiteralPath (Join-Path $fixture 'scenario-a-vue-taskboard.ps1')
    $env:LOCALAPPDATA = Join-Path $temporary 'account data'
    $account = Join-Path $env:LOCALAPPDATA 'VCP/account'
    New-Item -ItemType Directory -Path $account -Force | Out-Null
    $connection = Join-Path $account 'connection-fixture'
    New-ProviderFixture $connection
    @{ version = 1; connection = @{ status = 'connected'; model = 'fixture/model'; endpoint = 'fixture/endpoint'; evidence = $connection } } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $account 'setup-complete.json')
    $env:VCP_REUSE_MODEL = 'fixture/model'
    $env:VCP_REUSE_KEY = 'synthetic-reuse-key-not-a-real-secret'
    foreach ($name in 'VCP_PROVIDER_GENERATION', 'VCP_DENY_PROVIDER_CREDENTIALS', 'VCP_SCENARIO_CREDENTIAL_ENV') { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
    $pwsh = (Get-Process -Id $PID).Path
    $project = Join-Path $temporary 'existing project'
    New-Item -ItemType Directory -Path $project | Out-Null
    'preserve this file' | Set-Content -LiteralPath (Join-Path $project 'owner.txt')
    function Invoke-ReuseFixture([string]$Name, [string]$Mode = 'DryRun', [bool]$AuthorizeProcesses = $true) {
        $runRoot = Join-Path $temporary $Name
        $launchArgs = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $fixture 'run-cli-scenarios.ps1'),
            '-Scenario', 'A', '-Mode', $Mode, '-ProjectPath', $project, '-RunRoot', $runRoot, '-Vcp', $pwsh)
        if ($Mode -eq 'Full' -and $AuthorizeProcesses) { $launchArgs += '-AllowProcessPublish' }
        $output = (& $pwsh @launchArgs 2>&1 | Out-String)
        $code = $LASTEXITCODE
        Check (Test-Path -LiteralPath $runRoot) "Launcher did not create invocation root: $output"
        $roots = @(Get-ChildItem -LiteralPath $runRoot -Directory -Filter 'launch-*')
        Check ($roots.Count -eq 1) "Expected one $Name invocation; output: $output"
        $root = $roots[0].FullName
        $auditPath = Join-Path $root 'setup/logs/vcp-commands.jsonl'
        Check (Test-Path -LiteralPath $auditPath) "Missing audit: $output"
        $commands = @(Get-Content -LiteralPath $auditPath | ForEach-Object { $_ | ConvertFrom-Json -Depth 30 })
        $literal = Get-Content -LiteralPath (Join-Path $root 'setup/logs/vcp-commands.log') -Raw
        Check (-not $literal.Contains('synthetic-reuse-key')) 'Credential leaked into literal command log'
        Check (@($commands | Where-Object { $_.argv -contains 'provider' -or $_.argv -contains 'provider-complete' }).Count -eq 0) 'Provider qualification occurred'
        Check (@($commands | Where-Object label -eq 'models').Count -eq 1) 'Installed selection not read exactly once'
        $models = @($commands | Where-Object label -eq 'models')[0]
        Check ($models.argv -contains $project) 'Installed selection was not queried for selected project'
        return [pscustomobject]@{ Code = $code; Output = $output; Root = $root; Commands = $commands; Literal = $literal }
    }
    $before = [IO.File]::ReadAllBytes((Join-Path $connection 'snapshot.json'))
    $declined = Invoke-ReuseFixture 'full without permission' 'Full' $false
    Check ($declined.Code -eq 1 -and -not $declined.Output.Contains('PROVIDER_REUSE_CHILD_STARTED')) 'Full mode without process permission started a scenario'
    Check (@($declined.Commands | Where-Object label -eq 'credential-status').Count -eq 0) 'Permission refusal unnecessarily accessed credentials'
    $decision = Get-Content -LiteralPath (Join-Path $declined.Root 'setup/results/process-authorization.json') -Raw | ConvertFrom-Json
    Check (-not $decision.allowed -and $decision.source -eq 'not-authorized') 'Permission refusal was not retained'
    $success = Invoke-ReuseFixture 'reuse full' 'Full'
    Check ($success.Code -eq 0 -and $success.Output.Contains('PROVIDER_REUSE_CHILD_STARTED')) "Configured provider reuse failed: $($success.Output)"
    $received = Get-Content -LiteralPath (Join-Path $success.Root 'a-vue-taskboard/fixture-run/arguments.json') -Raw | ConvertFrom-Json
    Check ($received.generation -eq $connection) 'Did not reuse account setup connection evidence directly'
    Check ($received.allow_process_publish) 'Explicit process permission was not forwarded to scenario'
    $decision = Get-Content -LiteralPath (Join-Path $success.Root 'setup/results/process-authorization.json') -Raw | ConvertFrom-Json
    Check ($decision.allowed -and $decision.source -eq 'explicit -AllowProcessPublish') 'Explicit process permission was not recorded'
    Check ($received.credential_name -eq 'VCP_REUSE_KEY') 'Configured credential alias was not forwarded'
    Check ($received.project -eq $project -and $received.project_exists) 'Existing project was not passed to scenario'
    Check ((Get-Content -LiteralPath (Join-Path $project 'owner.txt') -Raw).Trim() -eq 'preserve this file') 'Existing project content was modified'
    Check ([Convert]::ToBase64String($before) -ceq [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $connection 'snapshot.json')))) 'Source snapshot was modified'
    Check ($success.Literal.Contains('results') -and $success.Literal.Contains('models')) 'Literal command log lacks command/results pointers'
    Check (Test-Path -LiteralPath (Join-Path $success.Root 'setup/results/provider-selection.json')) 'No selection provenance retained'
    $env:VCP_DENY_PROVIDER_CREDENTIALS = '1'
    $dry = Invoke-ReuseFixture 'reuse dry'
    Check ($dry.Code -eq 0) "DryRun unnecessarily required credentials: $($dry.Output)"
    Check (@($dry.Commands | Where-Object label -eq 'credential-status').Count -eq 0) 'DryRun inspected credentials'
    Remove-Item -LiteralPath Env:VCP_DENY_PROVIDER_CREDENTIALS
    # Extract a registered profile's JSON tokens and catalog bytes exactly.
    $profilePath = Join-Path $account 'profile-fixture.json'
    $snapshotText = [IO.File]::ReadAllText((Join-Path $connection 'snapshot.json'))
    $profileText = '{"workspace":' + ($project | ConvertTo-Json -Compress) + ',"catalog":' + ((Join-Path $connection 'endpoints.json') | ConvertTo-Json -Compress) + ',"provider":' + $snapshotText.Trim() + '}'
    [IO.File]::WriteAllText($profilePath, $profileText)
    $registration = Join-Path $account 'project-profile-fixture.json'
    @{ workspace = $project; profile = $profilePath } | ConvertTo-Json | Set-Content -LiteralPath $registration
    $registered = Invoke-ReuseFixture 'registered profile'
    Check ($registered.Code -eq 0) "Registered project profile reuse failed: $($registered.Output)"
    $selection = Get-Content -LiteralPath (Join-Path $registered.Root 'setup/results/provider-selection.json') -Raw | ConvertFrom-Json
    Check ($selection.profile -eq $profilePath) 'Registered project profile was not preferred'
    Check ([IO.File]::ReadAllText((Join-Path $selection.generation 'snapshot.json')).Trim() -ceq $snapshotText.Trim()) 'Embedded snapshot tokens changed during extraction'
    Check ((Get-FileHash -LiteralPath (Join-Path $selection.generation 'endpoints.json')).Hash -eq (Get-FileHash -LiteralPath (Join-Path $connection 'endpoints.json')).Hash) 'Catalog bytes changed during extraction'
    Remove-Item -LiteralPath $registration
    $env:VCP_REUSE_MODEL = 'different/model'
    $mismatch = Invoke-ReuseFixture 'mismatched model'
    Check ($mismatch.Code -eq 1 -and -not $mismatch.Output.Contains('PROVIDER_REUSE_CHILD_STARTED')) 'Unselected account connection model ran'
    $env:VCP_REUSE_MODEL = 'fixture/model'
    New-ProviderFixture $connection -1
    $expired = Invoke-ReuseFixture 'expired metadata'
    Check ($expired.Code -eq 1 -and -not $expired.Output.Contains('PROVIDER_REUSE_CHILD_STARTED')) 'Expired metadata started scenario'
    $failure = Get-Content -LiteralPath (Join-Path $expired.Root 'setup/results/provider-selection.json') -Raw | ConvertFrom-Json
    Check (($failure.rejected -join ' ').Contains('expired')) 'Expiry limitation not recorded'
    Check (@($expired.Commands | Where-Object label -eq 'credential-status').Count -eq 0) 'Metadata failure unnecessarily requested credentials'
    New-ProviderFixture $connection
    '{}' | Set-Content -LiteralPath (Join-Path $connection 'endpoints.json')
    $tampered = Invoke-ReuseFixture 'changed catalog'
    Check ($tampered.Code -eq 1) 'Changed captured catalog was accepted'
    $failure = Get-Content -LiteralPath (Join-Path $tampered.Root 'setup/results/provider-selection.json') -Raw | ConvertFrom-Json
    Check (($failure.rejected -join ' ').Contains('hash')) 'Catalog integrity failure not recorded'
    Write-Host "Provider reuse regressions passed: $checks checks; no native VCP, network or paid inference executed."
}
finally {
    foreach ($name in $savedEnvironment.Keys) {
        if ($null -eq $savedEnvironment[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
    }
    $resolved = [IO.Path]::GetFullPath($temporary)
    $prefix = $temporaryBase.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if ($resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $resolved) -like 'vcp-provider-reuse-tests-*') {
        if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
    }
}
